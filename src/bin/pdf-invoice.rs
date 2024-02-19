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

use std::{error::Error, future::pending};
use zbus::{ConnectionBuilder, DBusError, interface, zvariant};
use serde::{Serialize, Deserialize};
use chrono::prelude::*;
use chrono::Datelike;
use configparser::ini::Ini;

#[derive(DBusError, Debug)]
enum PDFError {
    #[zbus(error)]
    ZBus(zbus::Error),
    SVGLoadingError(String),
    SVGRenderingError(String),
    CairoError(String),
    IOError(String),
    MissingData(String),
    ArticleNameTooLong(String),
    PriceTooHigh(String),
    TooFarInTheFuture(String),
    StreamError(String),
}

impl From<rsvg::LoadingError> for PDFError {
    fn from(err: rsvg::LoadingError) -> PDFError {
            PDFError::SVGLoadingError(err.to_string())
    }
}

impl From<rsvg::RenderingError> for PDFError {
    fn from(err: rsvg::RenderingError) -> PDFError {
            PDFError::SVGRenderingError(err.to_string())
    }
}

impl From<cairo::Error> for PDFError {
    fn from(err: cairo::Error) -> PDFError {
            PDFError::CairoError(err.to_string())
    }
}

impl From<std::io::Error> for PDFError {
    fn from(err: std::io::Error) -> PDFError {
            PDFError::IOError(err.to_string())
    }
}

#[derive(Deserialize, Serialize, zvariant::Type, zvariant::Value, Clone, Default)]
struct InvoiceRecipient {
	firstname: String,
	lastname: String,
	street: String,
	postal_code: String,
	city: String,
	gender: String,
}

#[derive(Deserialize, Serialize, zvariant::Type, zvariant::Value, Clone)]
struct Product {
	ean: u64,
	name: String,
}

#[derive(Deserialize, Serialize, zvariant::Type, zvariant::Value, Clone)]
struct InvoiceEntry {
	timestamp: i64,
	product: Product,
	price: i32,
}

struct PDFInvoiceRenderer {
    datapath: String,
    longname: String,
    vat: String,
    addressrow: String,
    footer1: String,
    footer2: String,
    footer3: String,
    previous_tm: Option<chrono::DateTime<Local>>,
    invoice_id: String,
    invoice_date: i64,
    invoice_recipient: InvoiceRecipient,
    invoice_entries: Vec<InvoiceEntry>,
}

fn price_to_str(price: i32, with_euro: bool) -> String {
    let euro = price / 100;
    let cent = price % 100;
    let symbol = if with_euro { "€" } else { "" };
    format!("{euro},{cent:02}{symbol}")
}

impl PDFInvoiceRenderer {
	fn render_svg(&self, ctx: &cairo::Context, rect: &cairo::Rectangle, file: String) -> Result<(), PDFError> {
        let handle = rsvg::Loader::new().read_path(file)?;
        let renderer = rsvg::CairoRenderer::new(&handle);
        renderer.render_document(ctx, rect)?;
        Ok(())
	}

    fn draw_logo(&self, ctx: &cairo::Context) -> Result<(), PDFError> {
        let logo = format!("{}/logo.svg", self.datapath);
        let rect = cairo::Rectangle::new(366.0, 25.0, 170.0, 67.0);
		ctx.save()?;
		self.render_svg(ctx, &rect, logo)?;
		ctx.restore()?;
        Ok(())
    }

    fn draw_address(&self, ctx: &cairo::Context) -> Result<(), PDFError> {
        let addressrow = &self.addressrow;
        let name = format!("{} {}", self.invoice_recipient.firstname, self.invoice_recipient.lastname);
        let street = &self.invoice_recipient.street;
        let city = format!("{} {}", self.invoice_recipient.postal_code, self.invoice_recipient.city);

		ctx.save()?;
		ctx.set_source_rgb(0.0, 0.0, 0.0);
		ctx.set_line_width(1.0);

		/* upper fold mark (20 mm left, 85 mm width, 51.5 mm top) */
		ctx.move_to(56.69, 146.0);
		ctx.line_to(297.59, 146.0);
		ctx.stroke()?;

		/* actually LMSans8 */
		ctx.set_source_rgb(0.0, 0.0, 0.0);
		ctx.select_font_face("LMSans10", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
		ctx.set_font_size(8.45);

		ctx.move_to(56.5, 142.0);
		ctx.show_text(&addressrow)?;

		/* actually LMRoman12 */
		ctx.select_font_face("LMSans10", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
		ctx.set_font_size(12.3);

		ctx.move_to(56.5, 184.0);
		ctx.show_text(&name)?;

		ctx.move_to(56.5, 198.0);
		ctx.show_text(&street)?;

		/* actually LMRoman12 */
		ctx.select_font_face("LMSans10", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
		ctx.move_to(56.5, 227.0);
		ctx.show_text(&city)?;

		ctx.restore()?;

        Ok(())
    }

	fn draw_folding_marks(&self, ctx: &cairo::Context) -> Result<(), PDFError> {
		ctx.save()?;

		ctx.set_source_rgb(0.0, 0.0, 0.0);
		ctx.set_line_width(1.0);

		/* upper fold mark (105 mm) */
		ctx.move_to(10.0, 297.65);
		ctx.line_to(15.0, 297.65);
		ctx.stroke()?;

		/* middle fold mark (148.5 mm)*/
		ctx.move_to(10.0, 420.912);
		ctx.line_to(23.0, 420.912);
		ctx.stroke()?;

		/* lower fold mark (210 mm)*/
		ctx.move_to(10.0, 595.3);
		ctx.line_to(15.0, 595.3);
		ctx.stroke()?;

		ctx.restore()?;
        Ok(())
    }

	fn draw_footer(&self, ctx: &cairo::Context) -> Result<(), PDFError> {
        let footer = format!("{}/footer-line.svg", self.datapath);
        let rect = cairo::Rectangle::new(0.0, 0.0, 576.0, 29.0);
		ctx.save()?;
		ctx.translate(-20.0, 818.0);
		ctx.scale(1.42, 1.42);
		self.render_svg(ctx, &rect, footer)?;
		ctx.restore()?;
        Ok(())
	}

	fn draw_footer_text_left(&self, ctx: &cairo::Context) -> Result<(), PDFError> {
		ctx.save()?;
		ctx.move_to(64.0, 742.0);
		ctx.set_source_rgb(0.0, 0.0, 0.0);

		/* get pango layout */
		let layout = pangocairo::functions::create_layout(ctx);

		/* setup font */
		let mut font = pango::FontDescription::new();
		font.set_family("LMRoman8");
		font.set_size(6 * pango::SCALE);
		layout.set_font_description(Some(&font));

		/* left alignment */
		layout.set_alignment(pango::Alignment::Left);
		layout.set_wrap(pango::WrapMode::WordChar);

		/* set line spacing */
		layout.set_spacing(-2 * pango::SCALE);

		/* set page width */
		layout.set_width(140 * pango::SCALE);

		/* write invoice date */
		layout.set_markup(&self.footer1);

		/* render text */
		pangocairo::functions::update_layout(ctx, &layout);
		pangocairo::functions::show_layout(ctx, &layout);

		ctx.restore()?;
        Ok(())
	}

	fn draw_footer_text_middle(&self, ctx: &cairo::Context) -> Result<(), PDFError> {
		ctx.save()?;
		ctx.move_to(216.5, 742.0);
		ctx.set_source_rgb(0.0, 0.0, 0.0);

		/* get pango layout */
		let layout = pangocairo::functions::create_layout(ctx);

		/* setup font */
		let mut font = pango::FontDescription::new();
		font.set_family("LMRoman8");
		font.set_size(6 * pango::SCALE);
		layout.set_font_description(Some(&font));

		/* left alignment */
		layout.set_alignment(pango::Alignment::Left);
		layout.set_wrap(pango::WrapMode::WordChar);

		/* set line spacing */
		layout.set_spacing(-2 * pango::SCALE);

		/* set page width */
		layout.set_width(190 * pango::SCALE);

		/* write invoice date */
		layout.set_markup(&self.footer2);

		/* render text */
		pangocairo::functions::update_layout(ctx, &layout);
		pangocairo::functions::show_layout(ctx, &layout);

		ctx.restore()?;
        Ok(())
	}

	fn draw_footer_text_right(&self, ctx: &cairo::Context) -> Result<(), PDFError> {
		ctx.save()?;
		ctx.move_to(410.0, 742.0);
		ctx.set_source_rgb(0.0, 0.0, 0.0);

		/* get pango layout */
		let layout = pangocairo::functions::create_layout(ctx);

		/* setup font */
		let mut font = pango::FontDescription::new();
		font.set_family("LMRoman8");
		font.set_size(6 * pango::SCALE);
		layout.set_font_description(Some(&font));

		/* left alignment */
		layout.set_alignment(pango::Alignment::Left);
		layout.set_wrap(pango::WrapMode::WordChar);

		/* set line spacing */
		layout.set_spacing(-2 * pango::SCALE);

		/* set page width */
		layout.set_width(150 * pango::SCALE);

		/* write invoice date */
		layout.set_markup(&self.footer3);

		/* render text */
		pangocairo::functions::update_layout(ctx, &layout);
		pangocairo::functions::show_layout(ctx, &layout);

		ctx.restore()?;
        Ok(())
	}

	fn draw_date(&self, ctx: &cairo::Context) -> Result<(), PDFError> {
		ctx.save()?;
		ctx.move_to(56.5, 280.0);
		ctx.set_source_rgb(0.0, 0.0, 0.0);

		/* get pango layout */
		let layout = pangocairo::functions::create_layout(ctx);

		/* setup font */
		let mut font = pango::FontDescription::new();
		font.set_family("LMSans10");
		font.set_size(9 * pango::SCALE);
		layout.set_font_description(Some(&font));

		/* right alignment */
		layout.set_alignment(pango::Alignment::Right);
		layout.set_wrap(pango::WrapMode::WordChar);

		/* set page width */
		layout.set_width(446 * pango::SCALE);

		/* write invoice date */
        let invdate: chrono::DateTime<Utc> = chrono::DateTime::<Utc>::from_timestamp(self.invoice_date, 0).expect("invalid timestamp");
        let invdate: chrono::DateTime<Local> = chrono::DateTime::from(invdate);
		let date = invdate.format("%d.%m.%Y").to_string();
		layout.set_text(&date);

		/* render text */
		pangocairo::functions::update_layout(ctx, &layout);
		pangocairo::functions::show_layout(ctx, &layout);

		ctx.restore()?;
        Ok(())
	}

	fn draw_title(&self, ctx: &cairo::Context) -> Result<(), PDFError> {
		ctx.save()?;

		/* actually LMRoman12 */
		ctx.set_source_rgb(0.0, 0.0, 0.0);
		ctx.select_font_face("LMSans10", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
		ctx.set_font_size(12.9);

		ctx.move_to(56.5, 323.0);

        let text = format!("Rechnung Nr. {}", self.invoice_id);
		ctx.show_text(&text)?;

		ctx.restore()?;

        Ok(())
	}

	fn get_sum(&self) -> i32 {
		let mut sum = 0;
        for e in &self.invoice_entries {
			sum += e.price;
		}
		sum
	}

	fn get_address(&self) -> &'static str {
		if self.invoice_recipient.gender == "masculinum" {
			"Sehr geehrter Herr"
        } else if self.invoice_recipient.gender == "femininum" {
			"Sehr geehrte Frau"
        } else {
			"Moin"
        }
	}

	fn draw_first_page_text(&self, ctx: &cairo::Context) -> Result<(), PDFError> {
		ctx.save()?;
		ctx.move_to(56.5, 352.5);
		ctx.set_source_rgb(0.0, 0.0, 0.0);

		/* get pango layout */
		let layout = pangocairo::functions::create_layout(ctx);

		/* setup font */
		let mut font = pango::FontDescription::new();
		font.set_family("LMRoman12");
		font.set_size(9 * pango::SCALE);
		layout.set_font_description(Some(&font));

		/* left alignment */
		layout.set_alignment(pango::Alignment::Left);
		layout.set_wrap(pango::WrapMode::WordChar);

		/* set line spacing */
		layout.set_spacing((-2.1 * pango::SCALE as f32) as i32);

		/* set page width */
		layout.set_width(446 * pango::SCALE);

        let address = self.get_address();
		let sum = price_to_str(self.get_sum(), false);

		/* load text template */
        let template = format!("{}/pdf-template.txt", self.datapath);
        let text = std::fs::read_to_string(template)?;
        let text = text.replace("{{{ADDRESS}}}", address);
        let text = text.replace("{{{LASTNAME}}}", &self.invoice_recipient.lastname);
        let text = text.replace("{{{SUM}}}", &sum);
        let text = text.replace("{{{ORGANIZATION}}}", &self.longname);

        let text = if self.vat == "yes" {
            text.replace("{{{VAT}}}", "")
        } else {
            let template = format!("{}/vat.txt", self.datapath);
            let vattext = std::fs::read_to_string(template)?;
            text.replace("{{{VAT}}}", &vattext)
        };

        layout.set_markup(&text);

		/* render text */
		pangocairo::functions::update_layout(ctx, &layout);
		pangocairo::functions::show_layout(ctx, &layout);

		ctx.restore()?;
        Ok(())
	}

	fn draw_invoice_table_header(&self, ctx: &cairo::Context) -> Result<(), PDFError> {
		ctx.save()?;

		/* border & font color */
		ctx.set_source_rgb(0.0, 0.0, 0.0);

		/* line width of the border */
		ctx.set_line_width(0.8);

		/* header font */
		ctx.select_font_face("LMSans10", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
		ctx.set_font_size(12.0);

		/* borders */
		ctx.move_to(58.0, 50.0);
		ctx.line_to(530.0, 50.0);
		ctx.line_to(530.0, 65.0);
		ctx.line_to(58.0, 65.0);
		ctx.line_to(58.0, 50.0);
		ctx.move_to(120.0, 50.0);
		ctx.line_to(120.0, 65.0);
		ctx.move_to(180.0, 50.0);
		ctx.line_to(180.0, 65.0);
		ctx.move_to(480.0, 50.0);
		ctx.line_to(480.0, 65.0);
		ctx.stroke()?;

		/* header text */
		ctx.move_to(62.0, 61.5);
		ctx.show_text("Datum")?;
		ctx.move_to(124.0, 61.5);
		ctx.show_text("Uhrzeit")?;
		ctx.move_to(184.0, 61.5);
		ctx.show_text("Artikel")?;
		ctx.move_to(484.0, 61.5);
		ctx.show_text("Preis")?;

		ctx.restore()?;
        Ok(())
	}

	fn draw_invoice_table_footer(&self, ctx: &cairo::Context, y: f64) -> Result<(), PDFError> {
		ctx.save()?;

		/* border & font color */
		ctx.set_source_rgb(0.0, 0.0, 0.0);

		/* line width of the border */
		ctx.set_line_width(0.8);

		/* end of table is just a line */
		ctx.move_to(58.0, y);
		ctx.line_to(530.0, y);
		ctx.stroke()?;

		ctx.restore()?;
        Ok(())
	}

	fn draw_invoice_table_entry(&self, ctx: &cairo::Context, y: f64, e: &InvoiceEntry) -> Result<Option<(f64, chrono::DateTime<Local>)>, PDFError> {
		ctx.save()?;

		/* border & font color */
		ctx.set_source_rgb(0.0, 0.0, 0.0);

		/* y remains the same by default */
		let mut newy = y;

		/* generate strings for InvoiceEntry */
        let tm: chrono::DateTime<Utc> = chrono::DateTime::<Utc>::from_timestamp(e.timestamp, 0).expect("invalid timestamp");
        let tm: chrono::DateTime<Local> = chrono::DateTime::from(tm);
		let mut date = tm.format("%Y-%m-%d").to_string();
		let time = tm.format("%H:%M:%S").to_string();
		let price = price_to_str(e.price, true);

		if e.price > 999999 {
            let msg = "Prices > 9999.99€ are not supported!".to_string();
            return Err(PDFError::PriceTooHigh(msg));
		}

		if tm.year() > 9999 {
            let msg = "Years after 9999 are not supported!".to_string();
            return Err(PDFError::TooFarInTheFuture(msg));
		}

		/* if date remains the same do not add it again */
		if self.previous_tm != None &&
		   self.previous_tm.unwrap().year() == tm.year() &&
		   self.previous_tm.unwrap().month() == tm.month() &&
		   self.previous_tm.unwrap().day() == tm.day() {
			date = "".to_string();
		}

		/* move to position for article text */
		ctx.move_to(184.0, y);

		/* get pango layout */
		let layout = pangocairo::functions::create_layout(ctx);

		/* setup font */
		let mut font = pango::FontDescription::new();
		font.set_family("LMSans10");
		font.set_size(8 * pango::SCALE);
		layout.set_font_description(Some(&font));

		/* left alignment */
		layout.set_alignment(pango::Alignment::Left);
		layout.set_wrap(pango::WrapMode::WordChar);

		/* set line spacing */
		layout.set_spacing(-2 * pango::SCALE);

		/* set page width */
		layout.set_width(290 * pango::SCALE);

		/* write invoice date */
		layout.set_text(&e.product.name);

		/* get height of text */
		let (_w, h) = layout.size();
		let height = (h / pango::SCALE) as f64;

		/* verify that the text fits on the page */
		if 750.0 < y + height {
			return Ok(None);
        }

		/* render article text */
		pangocairo::functions::update_layout(ctx, &layout);
		pangocairo::functions::show_layout(ctx, &layout);

		/* render date, time (toy font api uses different y than pango) */
		ctx.select_font_face("LMSans10", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
		ctx.set_font_size(11.0);
		ctx.move_to(62.0, y+12.0);
		ctx.show_text(&date)?;
		ctx.move_to(124.0, y+12.0);
		ctx.show_text(&time)?;

		/* render price */
		ctx.move_to(484.0, y);
		let pricelayout = pangocairo::functions::create_layout(ctx);
		pricelayout.set_font_description(Some(&font));
		pricelayout.set_alignment(pango::Alignment::Right);
		pricelayout.set_width(42 * pango::SCALE);
		pricelayout.set_text(&price);
		pangocairo::functions::update_layout(ctx, &pricelayout);
		pangocairo::functions::show_layout(ctx, &pricelayout);

		/* add borders */
		ctx.set_line_width(0.8);
		ctx.move_to(58.0, y);
		ctx.line_to(58.0, y+height);
		ctx.move_to(120.0, y);
		ctx.line_to(120.0, y+height);
		ctx.move_to(180.0, y);
		ctx.line_to(180.0, y+height);
		ctx.move_to(480.0, y);
		ctx.line_to(480.0, y+height);
		ctx.move_to(530.0, y);
		ctx.line_to(530.0, y+height);
		ctx.stroke()?;

		ctx.restore()?;

		newy += height;

		Ok(Some((newy, tm)))
	}

	fn draw_invoice_table(&mut self, ctx: &cairo::Context) -> Result<(), PDFError> {
		ctx.save()?;

		self.draw_footer(ctx)?;
		self.draw_invoice_table_header(ctx)?;

		/* initial position for entries */
		let mut y = 65.0_f64;

		for entry in &self.invoice_entries {
            let result = self.draw_invoice_table_entry(ctx, y, entry)?;
            match result {
                Some((new_y, previous_tm)) => {
                    y = new_y;
                    self.previous_tm = Some(previous_tm);
                },
                None => {
                    /* entry could not be added, because end of page has been reached */
                    self.draw_invoice_table_footer(ctx, y)?;
                    ctx.show_page()?;

                    /* draw page footer & table header on new page */
                    self.draw_footer(ctx)?;
                    self.draw_invoice_table_header(ctx)?;

                    /* reset position */
                    y = 65.0_f64;

                    /* always print date on new pages */
                    self.previous_tm = None;

                    /* retry adding the entry */
                    let result = self.draw_invoice_table_entry(ctx, y, entry)?;
                    match result {
                        Some((new_y, previous_tm)) => {
                            y = new_y;
                            self.previous_tm = Some(previous_tm);
                        },
                        None => {
                            let msg = format!("Article name \"{}\" does not fit on a single page!", entry.product.name);
                            return Err(PDFError::ArticleNameTooLong(msg));
                        }
                    }
                }
			}
		}

		self.draw_invoice_table_footer(ctx, y)?;
		ctx.show_page()?;

		ctx.restore()?;
        Ok(())
	}

    fn generate(&mut self) -> Result<Vec<u8>, PDFError> {
        /* A4 sizes (in points, 72 DPI) */
        let width  = 595.27559; /* 210mm */
        let height = 841.88976; /* 297mm */

        let buffer: std::io::Cursor<Vec<u8>> = Default::default();
        let document = cairo::PdfSurface::for_stream(width, height, buffer)?;
        let ctx = cairo::Context::new(&document)?;

		if self.invoice_id.is_empty() {
            return Err(PDFError::MissingData("No invoice ID given!".to_string()));
        }

		if self.invoice_entries.is_empty() {
            return Err(PDFError::MissingData("No invoice data given!".to_string()));
        }

		if self.invoice_date == 0 {
            return Err(PDFError::MissingData("No invoice date given!".to_string()));
        }

		if self.invoice_recipient.firstname.is_empty() && self.invoice_recipient.lastname.is_empty() {
            return Err(PDFError::MissingData("No invoice recipient given!".to_string()));
        }

        self.draw_logo(&ctx)?;
        self.draw_address(&ctx)?;
        self.draw_folding_marks(&ctx)?;
        self.draw_footer(&ctx)?;
		self.draw_footer_text_left(&ctx)?;
		self.draw_footer_text_middle(&ctx)?;
		self.draw_footer_text_right(&ctx)?;
		self.draw_date(&ctx)?;
		self.draw_title(&ctx)?;
		self.draw_first_page_text(&ctx)?;
		ctx.show_page()?;

		/* following pages: invoice table */
		self.draw_invoice_table(&ctx)?;

		document.flush();
		let result = document.finish_output_stream();
        match result {
            Ok(boxedstream) => {
                let buffer = boxedstream.downcast::<std::io::Cursor<Vec<u8>>>();
                match buffer {
                    Ok(buffer) => {
                        Ok(buffer.into_inner())
                    },
                    Err(_) => {
                        Err(PDFError::StreamError("Failed to unbox stream".to_string()))
                    }
                }
            },
            Err(swe) => {
                Err(PDFError::StreamError(swe.error.to_string()))
            },
        }
    }

    fn clear(&mut self) {
        self.invoice_date = 0;
        self.invoice_id = String::new();
        self.invoice_recipient = InvoiceRecipient::default();
        self.invoice_entries.clear();
    }
}

struct PDFInvoice {
    renderer: PDFInvoiceRenderer,
}

#[interface(name = "io.mainframe.shopsystem.InvoicePDF")]
impl PDFInvoice {
    #[zbus(property)]
    async fn invoice_id(&self) -> &str {
        &self.renderer.invoice_id
    }

    #[zbus(property)]
    async fn set_invoice_id(&mut self, id: &str) {
        self.renderer.invoice_id = id.to_string();
    }

    #[zbus(property)]
    async fn invoice_date(&self) -> i64 {
        self.renderer.invoice_date
    }

    #[zbus(property)]
    async fn set_invoice_date(&mut self, date: i64) {
        self.renderer.invoice_date = date;
    }

    #[zbus(property)]
    async fn invoice_recipient(&self) -> InvoiceRecipient {
        self.renderer.invoice_recipient.clone()
    }

    #[zbus(property)]
    async fn set_invoice_recipient(&mut self, recipient: InvoiceRecipient) {
        self.renderer.invoice_recipient = recipient.clone();
    }

    #[zbus(property)]
    async fn set_invoice_entries(&mut self, invoice_entries: Vec<InvoiceEntry>) {
        self.renderer.invoice_entries = invoice_entries.clone();
    }

    #[zbus(property)]
    async fn invoice_entries(&self) -> Vec<InvoiceEntry> {
        self.renderer.invoice_entries.clone()
    }

    fn generate(&mut self) -> Result<Vec<u8>, PDFError> {
        self.renderer.generate()
    }

    fn clear(&mut self) {
        self.renderer.clear()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut cfg = Ini::new();
    cfg.load("/etc/shopsystem/config.ini").expect("failed to load config");
    let datapath = cfg.get("GENERAL", "datapath").unwrap_or("/usr/share/shopsystem/".to_string());
    let datapath = format!("{}/invoice", datapath);

    let longname = cfg.get("GENERAL", "longname").expect("config does not specify GENERAL longname");
    let vat = cfg.get("INVOICE", "vat").expect("config does not specify INVOICE vat");
    let addressrow = cfg.get("INVOICE", "addressrow").expect("config does not specify INVOICE addressrow");
    let footer1 = cfg.get("INVOICE", "footer1").expect("config does not specify INVOICE footer1").replace("\\n", "\n");
    let footer2 = cfg.get("INVOICE", "footer2").expect("config does not specify INVOICE footer2").replace("\\n", "\n");
    let footer3 = cfg.get("INVOICE", "footer3").expect("config does not specify INVOICE footer3").replace("\\n", "\n");

    let renderer = PDFInvoiceRenderer {
        datapath: datapath,
        longname: longname,
        addressrow: addressrow,
        footer1: footer1,
        footer2: footer2,
        footer3: footer3,
        vat: vat,
        previous_tm: None,
        invoice_id: String::new(),
        invoice_date: 0,
        invoice_recipient: InvoiceRecipient::default(),
        invoice_entries: Vec::new(),
    };

    let pdf = PDFInvoice { renderer: renderer };

    let _connection = ConnectionBuilder::system()?
        .name("io.mainframe.shopsystem.InvoicePDF")?
        .serve_at("/io/mainframe/shopsystem/invoicepdf", pdf)?
        .build()
        .await?;

    pending::<()>().await;

    Ok(())
}
