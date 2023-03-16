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
use zbus::{Connection, ConnectionBuilder, SignalContext, dbus_interface, dbus_proxy};
use rand::Rng;
use gstreamer::prelude::*;

fn get_files(dir: &str) -> std::io::Result<Vec<String>> {
    let mut entries = std::fs::read_dir(dir)?
        .map(|res| res.map(|e| e.file_name().into_string().unwrap()))
        .collect::<Result<Vec<_>, std::io::Error>>()?;
    entries.sort();
    Ok(entries)
}

fn get_random_file(dir: &str) -> std::io::Result<String> {
    let files = get_files(dir)?;
    let index = rand::thread_rng().gen_range(0usize..files.len());
    Ok(files[index].clone())
}

struct AudioPlayer {
    path: String,
    player: gstreamer::Element,
}

#[dbus_interface(name = "io.mainframe.shopsystem.AudioPlayer")]
impl AudioPlayer {
	fn get_user_themes(&mut self) -> Vec<String> {
        let userpath = self.path.clone() + "/user";
        match get_files(&userpath) {
            Ok(list) => list,
            Err(_) => Vec::new(),
        }
	}

	fn get_random_user_theme(&mut self) -> String {
        let userpath = self.path.clone() + "/user";
		match get_random_file(&userpath) {
            Ok(theme) => theme,
            Err(_) => String::new(),
        }
	}

    fn play_system(&mut self, file: &str) -> () {
        let fileuri = format!("file://{}/system/{}", self.path, file);
		self.player.set_state(gstreamer::State::Null).expect("Failed to set state");
		self.player.set_property("uri", &fileuri);
		println!("Play: {}", fileuri);
		self.player.set_state(gstreamer::State::Playing).expect("Failed to set state");
    }

    fn play_user(&mut self, theme: &str, name: &str) -> () {
		self.player.set_state(gstreamer::State::Null).expect("Failed to set state");
        let themedir = format!("{}/user/{}/{}", self.path, theme, name);
        let file = match get_random_file(&themedir) {
            Ok(file) => file,
            Err(_) => { return; },
        };
        let fileuri = format!("file://{}/{}", themedir, file);
		self.player.set_property("uri", &fileuri);
		println!("Play: {}", fileuri);
		self.player.set_state(gstreamer::State::Playing).expect("Failed to set state");
    }

    #[dbus_interface(signal)]
    async fn end_of_stream(ctxt: &SignalContext<'_>) -> zbus::Result<()>;
}

#[dbus_proxy(
    interface = "io.mainframe.shopsystem.Config",
    default_service = "io.mainframe.shopsystem.Config",
    default_path = "/io/mainframe/shopsystem/config"
)]
trait ShopConfig {
    async fn get_string(&self, section: &str, cfg: &str) -> zbus::Result<String>;
}

async fn cfg_get_str(section: &str, cfg: &str) -> zbus::Result<String> {
    let connection = Connection::system().await?;
    let proxy = ShopConfigProxy::new(&connection).await?;
    proxy.get_string(section, cfg).await
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    gstreamer::init()?;

    let mut path = cfg_get_str("GENERAL", "datapath").await?;
    path += "/sounds";

    let player = gstreamer::ElementFactory::make("playbin").name("player").build()?;
    let alsa = gstreamer::ElementFactory::make("alsasink").name("alsa").build()?;
    let bus = player.bus().expect("Missing bus");
    player.set_property("audio-sink", alsa);

    let audio = AudioPlayer {
        path: path,
        player: player,
    };

    let connection = ConnectionBuilder::system()?
        .name("io.mainframe.shopsystem.AudioPlayer")?
        .serve_at("/io/mainframe/shopsystem/audio", audio)?
        .build()
        .await?;

    let iface_ref = connection
        .object_server()
        .interface::<_, AudioPlayer>("/io/mainframe/shopsystem/audio").await?;

    // add_watch() and add_watch_local() do not work for some reason
    let _srcid = bus.set_sync_handler(move |_bus, msg| {
        use gstreamer::MessageView;
        match msg.view() {
            MessageView::Eos(..) => {
                let runtime = tokio::runtime::Runtime::new().unwrap();
                match runtime.block_on(AudioPlayer::end_of_stream(iface_ref.signal_context())) {
                    Ok(x) => x,
                    Err(_) => println!("Listener failure"),
                };
            },
            _ => {},
        }

        gstreamer::BusSyncReply::Drop
    });

    pending::<()>().await;

    Ok(())
}
