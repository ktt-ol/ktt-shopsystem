/* Copyright 2023, Sebastian Reichel <sre@mainframe.io>
 *
 * Permission to use, copy, modify, and/or distribute this software for any
 * purpose with or without fee is hereby granted, provided that the above
 * copyright notice and this permission notice appear in all copies.
 *
 * THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
 * WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
 * MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
 * ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
 * WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
 * ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
 * OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
 */
use pangocairo::glib::Bytes;
use std::error;
use std::path::PathBuf;
use std::fs::File;
use std::io::prelude::*;
use clap::{Parser};
use zbus::{Connection, DBusError, proxy, zvariant::Type};
use serde::{Serialize, Deserialize};
use barcoders::sym::code39::*;
use barcoders::generators::svg::*;
use chrono::prelude::*;

type Result<T> = std::result::Result<T, Box<dyn error::Error>>;

#[derive(DBusError, Debug)]
enum PDFError {
    StreamError(String),
}

#[derive(Type, Clone, Deserialize, Serialize)]
pub struct UserInfo {
	id: i32,
	firstname: String,
	lastname: String,
	email: String,
	gender: String,
	street: String,
	postal_code: String,
	city: String,
	pgp: String,
	joined_at: i64,
	disabled: bool,
	hidden: bool,
	sound_theme: String,
	rfid: Vec<String>,
}

#[proxy(
    interface = "io.mainframe.shopsystem.Database",
    default_service = "io.mainframe.shopsystem.Database",
    default_path = "/io/mainframe/shopsystem/database"
)]
trait ShopDB {
    async fn get_user_info(&self, userid: i32) -> zbus::Result<UserInfo>;
    async fn get_member_ids(&self) -> zbus::Result<Vec<i32>>;
}

async fn get_member_ids() -> zbus::Result<Vec<i32>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_member_ids().await
}

async fn get_user_info(uid: i32) -> zbus::Result<UserInfo> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_user_info(uid).await
}

#[derive(Parser, Debug)]
struct Cli {
    /// Path to output PDF file
    #[arg(short, long, default_value = "output.pdf")]
    output: PathBuf,
}

struct User {
    id: i32,
    firstname: String,
    lastname: String,
    barcode: String,
}

async fn get_user_list() -> Result<Vec<User>> {
    let memberids = get_member_ids().await?;
    let svg = SVG::new(200)
        .xdim(2)
        .foreground(Color::black())
        .background(Color::white());
    let mut users = Vec::new();

    for id in memberids {
        let user = get_user_info(id).await?;

        if user.disabled || user.hidden {
            continue;
        }

        let barcodedata = Code39::with_checksum(format!("USER {}", user.id))?.encode();
        let barcode = svg.generate(&barcodedata)?;

        users.push(User {
            id: user.id,
            firstname: user.firstname,
            lastname: user.lastname,
            barcode: barcode,
        });
    }

    Ok(users)
}

fn render_svg(ctx: &cairo::Context, rect: &cairo::Rectangle, data: &str) -> Result<()> {
    let bytes = Bytes::from(data.as_bytes());
    let stream = gio::MemoryInputStream::from_bytes(&bytes);
    let handle = rsvg::Loader::new().read_stream(&stream, None::<&gio::File>, None::<&gio::Cancellable>)?;
    let renderer = rsvg::CairoRenderer::new(&handle);
    renderer.render_document(ctx, rect)?;
    Ok(())
}

fn render_centered_text(ctx: &cairo::Context, x: f64, y: f64, w: i32, msg: &str) -> Result<()> {
    ctx.save()?;
    ctx.move_to(x, y);
    ctx.set_source_rgb(0.0, 0.0, 0.0);

    /* get pango layout */
    let layout = pangocairo::functions::create_layout(&ctx);

    /* setup font */
    let mut font = pango::FontDescription::new();
    font.set_family("LMRoman12");
    font.set_size(9 * pango::SCALE);
    layout.set_font_description(Some(&font));

    /* left alignment */
    layout.set_alignment(pango::Alignment::Center);
    layout.set_wrap(pango::WrapMode::WordChar);

    /* set line spacing */
    layout.set_spacing((-2.1 * pango::SCALE as f32) as i32);

    /* set page width */
    layout.set_width(w * pango::SCALE);

    /* write invoice date */
    layout.set_text(msg);

    /* render text */
    pangocairo::functions::update_layout(ctx, &layout);
    pangocairo::functions::show_layout(ctx, &layout);

    ctx.restore()?;
    Ok(())
}

fn render_user(ctx: &cairo::Context, user: &User, position: u32) -> Result<()> {
    let base = 50.0;

    let col = position % 2;
    let row = (position / 2) % 7;

    let y = base + row as f64 * (75.0+3.0*12.0);
    let x = if col == 0 { 50.0 } else { 347.637795 };
    let rect = cairo::Rectangle::new(x, y, 247.638, 75.0);
    render_svg(ctx, &rect, &user.barcode)?;

    let text_y = y + 75.0;
    let text_x = x + 15.0;
    let text_w = 220;

    let msg = format!("{} {} ({})", user.firstname, user.lastname, user.id);
    render_centered_text(ctx, text_x, text_y, text_w, &msg)?;

    Ok(())
}

fn render_header(ctx: &cairo::Context, timestamp: &str) -> Result<()> {
    ctx.save()?;

    ctx.move_to(24.0, 24.0);
    let header = format!("Shopsystem User List    (generated {})", timestamp);
    ctx.show_text(&header)?;

    ctx.restore()?;
    Ok(())
}

fn render_footer(ctx: &cairo::Context, page: u32, total_pages: u32) -> Result<()> {
    ctx.save()?;

    ctx.move_to(595.27559 - 75.0, 841.88976 - 12.0);
    let footer = format!("Page {} / {}", page, total_pages);
    ctx.show_text(&footer)?;

    ctx.restore()?;
    Ok(())
}

async fn draw_user_list(users: &Vec<User>) -> Result<Vec<u8>> {
    /* A4 sizes (in points, 72 DPI) */
    let width  = 595.27559; /* 210mm */
    let height = 841.88976; /* 297mm */

    let buffer: std::io::Cursor<Vec<u8>> = Default::default();
    let document = cairo::PdfSurface::for_stream(width, height, buffer)?;
    let ctx = cairo::Context::new(&document)?;

    let now: chrono::DateTime<Local> = Local::now();
    let timestamp = now.format("%Y-%m-%d %H:%M:%S").to_string();
    let mut position = 0;
    let mut page = 0;
    let total_pages = 1 + (users.len() as u32) / 14;

    for user in users {
        if position % 14 == 0 {
            if position > 0 {
                ctx.show_page()?;
            }
            page += 1;
            render_header(&ctx, &timestamp)?;
            render_footer(&ctx, page, total_pages)?;
        }

        render_user(&ctx, &user, position)?;
        position += 1;
    }

    document.flush();
    let result = document.finish_output_stream();
    match result {
        Ok(boxedstream) => {
            match boxedstream.downcast::<std::io::Cursor<Vec<u8>>>() {
                Ok(buffer) => Ok(buffer.into_inner()),
                Err(_) => Err(Box::new(PDFError::StreamError("Failed to unbox".to_string()))),
            }
        },
        Err(e) => {
            Err(Box::new(e.error))
        },
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();
    let users = get_user_list().await?;
    let pdfdata = draw_user_list(&users).await?;
    let mut output = File::create(args.output)?;
    output.write_all(&pdfdata)?;

    Ok(())
}
