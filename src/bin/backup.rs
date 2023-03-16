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

#[zbus::dbus_proxy(
    interface = "io.mainframe.shopsystem.Config",
    default_service = "io.mainframe.shopsystem.Config",
    default_path = "/io/mainframe/shopsystem/config"
)]
trait ShopConfig {
    async fn get_string(&self, section: &str, cfg: &str) -> zbus::Result<String>;
}

async fn cfg_get_str(section: &str, cfg: &str) -> zbus::Result<String> {
    let connection = zbus::Connection::system().await?;
    let proxy = ShopConfigProxy::new(&connection).await?;
    proxy.get_string(section, cfg).await
}

#[zbus::dbus_proxy(
    interface = "io.mainframe.shopsystem.Mailer",
    default_service = "io.mainframe.shopsystem.Mailer",
    default_path = "/io/mainframe/shopsystem/mailer"
)]
trait ShopMailer {
    fn create_mail(&self) -> zbus::Result<String>;
    fn delete_mail(&self, path: String) -> zbus::Result<()>;
    fn send_mail(&self, path: String) -> zbus::Result<()>;
}

#[derive(serde::Deserialize, serde::Serialize, zbus::zvariant::Type, zbus::zvariant::Value, Clone)]
pub struct MailContact {
	name: String,
	email: String,
}

#[derive(serde::Deserialize, serde::Serialize, zbus::zvariant::Type)]
pub enum RecipientType {
	To,
	Cc,
	Bcc
}

#[derive(serde::Deserialize, serde::Serialize, PartialEq, zbus::zvariant::Type)]
pub enum MessageType {
	Plain,
	Html
}

#[zbus::dbus_proxy(
    interface = "io.mainframe.shopsystem.Mail",
    default_service = "io.mainframe.shopsystem.Mail"
)]
trait ShopMail {
    fn set_from(&self, from: MailContact) -> zbus::Result<()>;
    fn set_subject(&self, subject: String) -> zbus::Result<()>;

    fn add_recipient(&self, contact: MailContact, recpttype: RecipientType) -> zbus::Result<()>;
    fn set_main_part(&self, text: String, msgtype: MessageType) -> zbus::Result<()>;
    fn add_attachment(&self, filename: String, content_type: String, data: Vec<u8>) -> zbus::Result<()>;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dbus_connection = zbus::Connection::system().await?;
    let mailer = ShopMailerProxy::new(&dbus_connection).await?;

    let mailpath = mailer.create_mail().await?;
    let mail = ShopMailProxy::builder(&dbus_connection).path(mailpath.clone())?.build().await?;

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    let dbfile = cfg_get_str("DATABASE", "file").await?;
    let dbdata = std::fs::read(dbfile)?;

    mail.set_from(MailContact {name: "KtT Shopsystem".to_string(), email: "shop@kreativitaet-trifft-technik.de".to_string()}).await?;
    mail.add_recipient(MailContact {name: "KtT Shopsystem Backups".to_string(), email: "shop-backup@kreativitaet-trifft-technik.de".to_string()}, RecipientType::To).await?;
    mail.set_subject(format!("Backup KtT-Shopsystem {now}")).await?;
    mail.set_main_part("You can find a backup of 'shop.db' attached to this mail.".to_string(), MessageType::Plain).await?;
    mail.add_attachment("shop.db".to_string(), "application/x-sqlite3".to_string(), dbdata).await?;

    mailer.send_mail(mailpath).await?;
    Ok(())
}
