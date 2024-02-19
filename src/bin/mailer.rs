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
use zbus::{Connection, ConnectionBuilder, interface, DBusError};
use serde::{Serialize, Deserialize};
use lettre::transport::smtp::authentication::Credentials;
use std::collections::HashSet;
use lettre::AsyncTransport;
use configparser::ini::Ini;

#[derive(Debug, DBusError)]
enum MailerError {
    NoMail(String),
    DBusError(String),
    AddressError(String),
    LettreError(String),
    ContentTypeErr(String),
    SMTPError(String),
}

impl From<zbus::Error> for MailerError {
    fn from(err: zbus::Error) -> MailerError {
        MailerError::DBusError(err.to_string())
    }
}

impl From<zbus::zvariant::Error> for MailerError {
    fn from(err: zbus::zvariant::Error) -> MailerError {
        MailerError::DBusError(err.to_string())
    }
}

impl From<lettre::address::AddressError> for MailerError {
    fn from(err: lettre::address::AddressError) -> MailerError {
        MailerError::AddressError(err.to_string())
    }
}

impl From<lettre::error::Error> for MailerError {
    fn from(err: lettre::error::Error) -> MailerError {
        MailerError::LettreError(err.to_string())
    }
}

impl From<lettre::message::header::ContentTypeErr> for MailerError {
    fn from(err: lettre::message::header::ContentTypeErr) -> MailerError {
        MailerError::ContentTypeErr(err.to_string())
    }
}

impl From<lettre::transport::smtp::Error> for MailerError {
    fn from(err: lettre::transport::smtp::Error) -> MailerError {
        MailerError::SMTPError(err.to_string())
    }
}

impl TryFrom<MailContact> for lettre::message::Mailbox {
    type Error = lettre::address::AddressError;

    fn try_from(contact: MailContact) -> Result<lettre::message::Mailbox, lettre::address::AddressError> {
        let address = contact.email.parse::<lettre::Address>()?;
        Ok(lettre::message::Mailbox::new(Some(contact.name.clone()), address))
    }
}

#[derive(Deserialize, Serialize, zbus::zvariant::Type)]
enum MessageType {
	Plain,
	Html
}

#[derive(Deserialize, Serialize, zbus::zvariant::Type, zbus::zvariant::Value, Clone)]
struct MailContact {
	name: String,
	email: String,
}

#[derive(Deserialize, Serialize, zbus::zvariant::Type, zbus::zvariant::Value, Clone)]
struct MailDate {
	date: u64,
	timezone: String,
}

#[derive(Deserialize, Serialize, zbus::zvariant::Type)]
enum RecipientType {
	To,
	Cc,
	Bcc
}

struct MailRecipient {
    contact: MailContact,
    recpttype: RecipientType,
}

struct MailAttachment {
    filename: String,
    content_type: String,
    data: Vec<u8>,
}

impl TryFrom<&MailAttachment> for lettre::message::SinglePart {
    type Error = lettre::message::header::ContentTypeErr;

    fn try_from(a: &MailAttachment) -> Result<lettre::message::SinglePart, Self::Error> {
        let content_type = lettre::message::header::ContentType::parse(&a.content_type)?;
        Ok(lettre::message::Attachment::new(a.filename.clone()).body(a.data.clone(), content_type))
    }
}

struct Mail {
    from: MailContact,
    subject: String,
    message_id: String,
    reply_to: String,
    date: MailDate,
    text_plain: Option<String>,
    text_html: Option<String>,
    recipients: Vec<MailRecipient>,
    attachments: Vec<MailAttachment>,
}

impl Mail {
    fn generate(&self) -> Result<lettre::Message, MailerError> {
        let mut m = lettre::Message::builder()
            .user_agent("KtT Shopsystem".to_string())
            .from(self.from.clone().try_into()?)
            .subject(self.subject.clone());
        for recipient in &self.recipients {
            m = match &recipient.recpttype {
                RecipientType::To => m.to(recipient.contact.clone().try_into()?),
                RecipientType::Cc => m.cc(recipient.contact.clone().try_into()?),
                RecipientType::Bcc => m.bcc(recipient.contact.clone().try_into()?),
            }
        }

        if !self.reply_to.is_empty() {
            m = m.reply_to(self.reply_to.parse()?);
        }

        m = if !self.message_id.is_empty() {
            m.message_id(Some(self.message_id.clone()))
        } else {
            m.message_id(None)
        };

        m = if self.date.date != 0 {
            let time = std::time::SystemTime::UNIX_EPOCH.checked_add(std::time::Duration::new(self.date.date, 0));
            if time.is_some() {
                m.date(time.unwrap())
            } else {
                m.date_now()
            }
        } else {
            m.date_now()
        };

        if self.attachments.is_empty() {
            return if self.text_html.is_none() && self.text_plain.is_none() {
                Ok(m.body(String::new())?)
            } else if self.text_html.is_some() && self.text_plain.is_none() {
                let part = lettre::message::SinglePart::builder()
                    .header(lettre::message::header::ContentType::TEXT_HTML)
                    .body(String::from(self.text_html.as_ref().unwrap().clone()));
                Ok(m.singlepart(part)?)
            } else if self.text_html.is_none() && self.text_plain.is_some() {
                let part = lettre::message::SinglePart::builder()
                    .header(lettre::message::header::ContentType::TEXT_PLAIN)
                    .body(String::from(self.text_plain.as_ref().unwrap().clone()));
                Ok(m.singlepart(part)?)
            } else {
                Ok(m.multipart(lettre::message::MultiPart::alternative_plain_html(
                            self.text_plain.as_ref().unwrap().clone(),
                            self.text_html.as_ref().unwrap().clone()
                ))?)
            };
        }

        let mp = lettre::message::MultiPart::mixed();
        let mut mp = if self.text_html.is_some() && self.text_plain.is_some() {
            mp.multipart(
                lettre::message::MultiPart::alternative()
                    .singlepart(lettre::message::SinglePart::plain(self.text_plain.as_ref().unwrap().clone()))
                    .singlepart(lettre::message::SinglePart::html(self.text_html.as_ref().unwrap().clone()))
            )
        } else if self.text_html.is_some() {
            mp.singlepart(lettre::message::SinglePart::html(self.text_html.as_ref().unwrap().clone()))
        } else if self.text_plain.is_some() {
            mp.singlepart(lettre::message::SinglePart::plain(self.text_plain.as_ref().unwrap().clone()))
        } else {
            mp.singlepart(lettre::message::SinglePart::plain(String::new()))
        };

        for attachment in &self.attachments {
            mp = mp.singlepart(attachment.try_into()?);
        }

        Ok(m.multipart(mp)?)
    }
}

struct DBusMail {
    mail: Mail,
}

#[interface(name = "io.mainframe.shopsystem.Mail")]
impl DBusMail {
    #[zbus(property)]
    async fn from(&self) -> MailContact {
        self.mail.from.clone()
    }

    #[zbus(property)]
    async fn set_from(&mut self, from: MailContact) {
        self.mail.from = from;
    }

    #[zbus(property)]
    async fn subject(&self) -> String {
        self.mail.subject.clone()
    }

    #[zbus(property)]
    async fn set_subject(&mut self, subject: String) {
        self.mail.subject = subject;
    }

    #[zbus(property)]
    async fn message_id(&self) -> String {
        self.mail.message_id.clone()
    }

    #[zbus(property)]
    async fn set_message_id(&mut self, message_id: String) {
        self.mail.message_id = message_id;
    }

    #[zbus(property)]
    async fn reply_to(&self) -> String {
        self.mail.reply_to.clone()
    }

    #[zbus(property)]
    async fn set_reply_to(&mut self, reply_to: String) {
        self.mail.reply_to = reply_to;
    }

    #[zbus(property)]
    async fn date(&self) -> MailDate {
        self.mail.date.clone()
    }

    #[zbus(property)]
    async fn set_date(&mut self, date: MailDate) {
        self.mail.date = date;
    }

    fn set_main_part(&mut self, text: String, msgtype: MessageType) -> () {
        match msgtype {
            MessageType::Plain => {
                self.mail.text_plain = Some(text);
            },
            MessageType::Html => {
                self.mail.text_html = Some(text);
            },
        }
    }

    fn add_recipient(&mut self, contact: MailContact, recpttype: RecipientType) -> () {
        self.mail.recipients.push(MailRecipient {
            contact: contact,
            recpttype: recpttype,
        });
    }

    fn add_attachment(&mut self, filename: String, content_type: String, data: Vec<u8>) -> () {
        self.mail.attachments.push(MailAttachment {
            filename: filename,
            content_type: content_type,
            data: data,
        });
    }

}

struct Mailer {
    server: String,
    port: u16,
    credentials: Credentials,
    starttls: bool,
    mailcounter: u64,
    mails: HashSet<String>,
    mailconnection: zbus::Connection,
}

#[interface(name = "io.mainframe.shopsystem.Mailer")]
impl Mailer {

    async fn create_mail(&mut self) -> Result<String, MailerError> {
		let path = format!("/io/mainframe/shopsystem/mail/{}", self.mailcounter);
        let dbuspath = zbus::zvariant::ObjectPath::try_from(path.clone())?;

		let mail = DBusMail {
            mail: Mail {
                from: MailContact {
                    name: String::new(),
                    email: String::new(),
                },
                subject: String::new(),
                message_id: String::new(),
                reply_to: String::new(),
                date: MailDate {
                    date: 0,
                    timezone: String::new(),
                },
                text_plain: None,
                text_html: None,
                recipients: Vec::new(),
                attachments: Vec::new(),
            },
        };

        self.mailconnection.request_name("io.mainframe.shopsystem.Mail").await?;
        self.mailconnection.object_server().at(&dbuspath, mail).await?;

        self.mails.insert(path.clone());
		self.mailcounter += 1;
        Ok(path)
    }

    async fn delete_mail(&mut self, path: String) -> Result<(), MailerError> {
        if !self.mails.contains(&path) {
            return Err(MailerError::NoMail("No such mail".to_string()));
        }

        let dbuspath = zbus::zvariant::ObjectPath::try_from(path.clone())?;
        let result = self.mailconnection.object_server().remove::<DBusMail, &zbus::zvariant::ObjectPath>(&dbuspath).await?;

        if !result {
            return Err(MailerError::NoMail("Failed to remove mail".to_string()));
        }

		self.mails.remove(&path);
        Ok(())
    }

    async fn send_mail(&self, path: String) -> Result<(), MailerError> {
        if !self.mails.contains(&path) {
            return Err(MailerError::NoMail("No such mail".to_string()));
        }

        let dbuspath = zbus::zvariant::ObjectPath::try_from(path.clone())?;
        let srv = self.mailconnection.object_server();
        let iface = srv.interface::<_, DBusMail>(&dbuspath).await?;
        let mail = &iface.get_mut().await.mail;
        let mail = mail.generate()?;

        let smtp = if self.server == "127.0.0.1" || self.server == "localhost" {
            lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::unencrypted_localhost()
        } else {
            if self.starttls {
                lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::starttls_relay(&self.server)
            } else {
                lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::relay(&self.server)
            }?
                .port(self.port)
                .credentials(self.credentials.clone())
                .build()
        };

        match smtp.send(mail).await {
            Ok(_) => Ok(()),
            Err(e) => Err(MailerError::SMTPError(format!("Failed to send mail: {}", e.to_string()))),
        }
    }

}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut cfg = Ini::new();
    cfg.load("/etc/shopsystem/config.ini").expect("failed to load config");
    let username = cfg.get("MAIL", "username").unwrap_or(String::new());
    let password = cfg.get("MAIL", "password").unwrap_or(String::new());
    let servername = cfg.get("MAIL", "server").expect("config is missing MAIL server");
    let serverport = cfg.getint("MAIL", "port")?.unwrap_or(25);
    let starttls = cfg.getbool("MAIL", "starttls")?.unwrap_or(true);
    let credentials = Credentials::new(username, password);
    let mailer = Mailer {
        server: servername,
        port: serverport as u16,
        credentials: credentials,
        starttls: starttls,
        mailcounter: 0,
        mails: HashSet::new(),
        mailconnection: Connection::system().await?,
    };

    let _connection = ConnectionBuilder::system()?
        .name("io.mainframe.shopsystem.Mailer")?
        .serve_at("/io/mainframe/shopsystem/mailer", mailer)?
        .build()
        .await?;

    pending::<()>().await;

    Ok(())
}
