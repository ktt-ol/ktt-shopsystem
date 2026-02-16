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
use std::error::Error;
use clap::{ArgGroup, Parser};
use zbus::{Connection, proxy, zvariant::Type};
use serde::{Serialize, Deserialize};
use chrono::{Datelike, offset::TimeZone, prelude::*};
use unicode_segmentation::UnicodeSegmentation;
use configparser::ini::Ini;

#[derive(Debug)]
enum InvoicerError {
    DBusError(String),
    IOError(String),
}

impl core::fmt::Display for InvoicerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DBusError(error) => 
                write!(f, "{}", error),
            Self::IOError(error) => 
                write!(f, "{}", error),
        }
    }
}

impl std::error::Error for InvoicerError {}

impl From<zbus::Error> for InvoicerError {
    fn from(err: zbus::Error) -> Self {
        Self::DBusError(err.to_string())
    }
}

impl From<std::io::Error> for InvoicerError {
    fn from(err: std::io::Error) -> Self {
        Self::IOError(err.to_string())
    }
}

#[derive(Debug)]
struct Timespan {
	from: i64,
	to: i64,
}

struct InvoiceData {
	pdffilename: String,
	pdfdata: Vec<u8>,
	plain: String,
	html: String,
}

#[derive(Parser, Debug)]
#[clap(group(
            ArgGroup::new("type")
                .args(&["all", "single"]),
       ),
       group(
            ArgGroup::new("mode")
                .required(true)
                .args(&["day", "month"]),
       ),
       group(
            ArgGroup::new("singlegrp")
                .args(&["single"])
                .requires("user"),
       ),
       group(
            ArgGroup::new("allgrp")
                .args(&["all"])
                .conflicts_with("user"),
       ),

)]
struct Cli {
    /// Generate invoices for all users (default)
    #[clap(long, default_value_t = true)]
    all: bool,
    /// Generate invoice for a single user
    #[clap(long, default_value_t = false)]
    single: bool,
    /// Generate daily/temporary invoice
    #[clap(long)]
    day: bool,
    /// Generate monthly/final invoice
    #[clap(long)]
    month: bool,
    /// Timestamp for the invoice (default=now)
    #[arg(short, long)]
    timestamp: Option<i64>,
    /// UserID
    #[arg(short, long)]
    user: Option<i32>,
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

#[derive(Deserialize, Serialize, zbus::zvariant::Type, zbus::zvariant::Value, Clone, Default)]
pub struct InvoiceRecipient {
	firstname: String,
	lastname: String,
	street: String,
	postal_code: String,
	city: String,
	gender: String,
}

#[derive(Deserialize,Serialize, zbus::zvariant::Type, zbus::zvariant::Value)]
pub struct Product {
	ean: i64,
	name: String,
}

#[derive(Deserialize, Serialize, zbus::zvariant::Type, zbus::zvariant::Value)]
pub struct InvoiceEntry {
	timestamp: i64,
	product: Product,
	price: i32,
}

#[derive(Deserialize, Serialize, PartialEq, Copy, Clone, zbus::zvariant::Type)]
pub enum MessageType {
	Plain,
	Html
}

#[proxy(
    interface = "io.mainframe.shopsystem.Database",
    default_service = "io.mainframe.shopsystem.Database",
    default_path = "/io/mainframe/shopsystem/database"
)]
trait ShopDB {
    async fn get_user_info(&self, userid: i32) -> zbus::Result<UserInfo>;
    async fn get_invoice(&self, userid: i32, from: i64, to: i64) -> zbus::Result<Vec<InvoiceEntry>>;
    async fn get_user_invoice_sum(&self, userid: i32, from: i64, to: i64) -> zbus::Result<i32>;
    async fn get_users_with_sales(&self, timestamp_from: i64, timestamp_to: i64) -> zbus::Result<Vec<i32>>;
}

async fn get_user_info(uid: i32) -> zbus::Result<UserInfo> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_user_info(uid).await
}

async fn get_invoice(uid: i32, start: i64, stop: i64) -> zbus::Result<Vec<InvoiceEntry>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_invoice(uid, start, stop).await
}

async fn get_user_invoice_sum(uid: i32, start: i64, stop: i64) -> zbus::Result<i32> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_user_invoice_sum(uid, start, stop).await
}

async fn get_users_with_sales(start: i64, stop: i64) -> zbus::Result<Vec<i32>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_users_with_sales(start, stop).await
}

#[proxy(
    interface = "io.mainframe.shopsystem.InvoicePDF",
    default_service = "io.mainframe.shopsystem.InvoicePDF",
    default_path = "/io/mainframe/shopsystem/invoicepdf"
)]
trait ShopPDF {
    #[zbus(property)]
    fn set_invoice_id(&self, id: &str) -> zbus::Result<()>;
    #[zbus(property)]
    fn set_invoice_date(&self, date: i64) -> zbus::Result<()>;
    #[zbus(property)]
    fn set_invoice_recipient(&self, recipient: InvoiceRecipient) -> zbus::Result<()>;
    #[zbus(property)]
    fn set_invoice_entries(&self, invoice_entries: Vec<InvoiceEntry>) -> zbus::Result<()>;

    fn generate(&self) -> zbus::Result<Vec<u8>>;
    fn clear(&self) -> zbus::Result<()>;
}

#[proxy(
    interface = "io.mainframe.shopsystem.Mailer",
    default_service = "io.mainframe.shopsystem.Mailer",
    default_path = "/io/mainframe/shopsystem/mailer"
)]
trait ShopMailer {
    fn create_mail(&self) -> zbus::Result<String>;
    fn delete_mail(&self, path: String) -> zbus::Result<()>;
    fn send_mail(&self, path: String) -> zbus::Result<()>;
}

#[derive(Deserialize, Serialize, zbus::zvariant::Type, zbus::zvariant::Value, Clone)]
pub struct MailContact {
	name: String,
	email: String,
}

#[derive(Deserialize, Serialize, zbus::zvariant::Type)]
pub enum RecipientType {
	To,
	Cc,
	Bcc
}

#[proxy(
    interface = "io.mainframe.shopsystem.Mail",
    default_service = "io.mainframe.shopsystem.Mail"
)]
trait ShopMail {
    #[zbus(property)]
    fn set_from(&self, from: MailContact) -> zbus::Result<()>;
    #[zbus(property)]
    fn set_subject(&self, subject: String) -> zbus::Result<()>;

    fn add_recipient(&self, contact: MailContact, recpttype: RecipientType) -> zbus::Result<()>;
    fn set_main_part(&self, text: String, msgtype: MessageType) -> zbus::Result<()>;
    fn add_attachment(&self, filename: String, content_type: String, data: Vec<u8>) -> zbus::Result<()>;
}

struct Invoicer {
	datadir: String,
	mailfromaddress: String,
	treasurermailaddress: String,
	shortname: String,
	spacename: String,
	jverein_membership_number: String,
}

impl Invoicer {

	async fn send_invoices(&self, temporary: bool, timestamp: i64, limit_to_user: Option<i32>) -> Result<(), InvoicerError> {
		let prevtimestamp;
		let due_date_string;

        let dbus_connection = Connection::system().await?;
        let mailer = ShopMailerProxy::new(&dbus_connection).await?;

        let ts: chrono::DateTime<Utc> = chrono::DateTime::<Utc>::from_timestamp(timestamp, 0).expect("invalid timestamp");
        let ts: chrono::DateTime<Local> = chrono::DateTime::from(ts);

		if !temporary {
            let prevts = ts - chrono::Months::new(1);
            prevtimestamp = prevts.timestamp();

            let duets = ts + chrono::Days::new(10);
            due_date_string = duets.format("%d.%m.%Y").to_string();
		} else {
            let prevts = ts - chrono::Days::new(1);
            prevtimestamp = prevts.timestamp();

		    due_date_string = String::new();
        }

		let ts = Self::get_timespan(temporary, prevtimestamp);
		let tst = Self::get_timespan(false, prevtimestamp);
		let mut number = 0;

        let start: chrono::DateTime<Utc> = chrono::DateTime::<Utc>::from_timestamp(ts.from, 0).expect("invalid timestamp");
        let start: chrono::DateTime<Local> = chrono::DateTime::from(start);
        let stop: chrono::DateTime<Utc> = chrono::DateTime::<Utc>::from_timestamp(ts.to, 0).expect("invalid timestamp");
        let stop: chrono::DateTime<Local> = chrono::DateTime::from(stop);
		let startstring = start.format("%d.%m.%Y %H:%M:%S").to_string();
		let stopstring  = stop.format("%d.%m.%Y %H:%M:%S").to_string();

        let sendername = format!("{} Shopsystem", self.shortname);

		/* title */
        let mailtitle = if temporary { "Getränkezwischenstand" } else { "Getränkerechnung" };
        let mailtitle = format!("{} {} - {}", mailtitle, startstring, stopstring);

		let users = get_users_with_sales(ts.from, ts.to).await?;

		println!("{}\n{:?}\nUsers: {}", mailtitle, ts, users.len() );

		let treasurer_path = mailer.create_mail().await?;
        let treasurer_mail = ShopMailProxy::builder(&dbus_connection).path(treasurer_path.clone())?.build().await?;
		treasurer_mail.set_from(MailContact {name: sendername.clone(), email: self.mailfromaddress.clone()}).await?;
		treasurer_mail.set_subject(mailtitle.clone()).await?;
		treasurer_mail.add_recipient(MailContact {name: "Schatzmeister".to_string(), email: self.treasurermailaddress.clone()}, RecipientType::To).await?;

		let mut csvinvoicedata = String::new();
		let mut csvjvereininvoicedata = if self.jverein_membership_number == "extern" {
			"Ext_Mitglieds_Nr;Betrag;Buchungstext;Fälligkeit;Intervall;Endedatum\n".to_string()
		} else {
			"Mitglieds_Nr;Betrag;Buchungstext;Fälligkeit;Intervall;Endedatum\n".to_string()
		};

		for userid in users {
			number += 1;

            let invoiceid = format!("SH{}5{:03}", start.format("%Y%m").to_string(), number);
			let invoicedata = self.generate_invoice(temporary, timestamp, userid, &invoiceid).await?;
			let userdata = get_user_info(userid).await?;
			let total_sum = get_user_invoice_sum(userid, tst.from, tst.to).await?;

            /*
             * Even when limited to one user we need to process all for two reasons:
             *  1. The Invoice ID will incorrectly be 0001 otherwise
             *  2. The CSV for the treasurer should always have all entries
             */
            if limit_to_user.is_none() || limit_to_user.unwrap() == userid {
                println!("{} ({} {})...", userdata.id, &userdata.firstname, &userdata.lastname);

                let mail_path = mailer.create_mail().await?;
                let mail = ShopMailProxy::builder(&dbus_connection).path(mail_path.clone())?.build().await?;
                mail.set_from(MailContact {name: sendername.clone(), email: self.mailfromaddress.clone()}).await?;
                mail.set_subject(mailtitle.clone()).await?;
                let recipientname = format!("{} {}", &userdata.firstname, &userdata.lastname);
                mail.add_recipient(MailContact {name: recipientname, email: userdata.email.clone()}, RecipientType::To).await?;

                if !temporary {
                    mail.add_attachment(invoicedata.pdffilename.clone(), "application/pdf".to_string(), invoicedata.pdfdata.clone()).await?;
                    treasurer_mail.add_attachment(invoicedata.pdffilename, "application/pdf".to_string(), invoicedata.pdfdata).await?;
                }

                mail.set_main_part(invoicedata.plain, MessageType::Plain).await?;
                mail.set_main_part(invoicedata.html, MessageType::Html).await?;
                mailer.send_mail(mail_path.clone()).await?;
            }

			if !temporary {
                let tmp = format!("{0},{1},{2},{invoiceid},{total_sum}\n", userdata.id, userdata.lastname, userdata.firstname);
                csvinvoicedata.push_str(&tmp);

                let tmp = format!("{0};{total_sum};Shopsystem Rechnung Nummer {invoiceid};{due_date_string};0;{due_date_string}\n", userdata.id);
                csvjvereininvoicedata.push_str(&tmp);
			}
		}

		if !temporary {
            let text = self.get_treasurer_text()?;
			treasurer_mail.set_main_part(text, MessageType::Plain).await?;
			treasurer_mail.add_attachment("invoice.csv".to_string(), "text/csv; charset=utf-8".to_string(), csvinvoicedata.into()).await?;
			treasurer_mail.add_attachment("jvereininvoice.csv".to_string(), "text/csv; charset=utf-8".to_string(), csvjvereininvoicedata.into()).await?;
			mailer.send_mail(treasurer_path).await?;
		} else {
			mailer.delete_mail(treasurer_path).await?;
        }

        Ok(())
	}

	async fn generate_invoice(&self, temporary: bool, timestamp: i64, userid: i32, invoiceid: &str) -> zbus::Result<InvoiceData> {
        let prevtimestamp = if temporary {
            let ts: chrono::DateTime<Utc> = chrono::DateTime::<Utc>::from_timestamp(timestamp, 0).expect("invalid timestamp");
            let ts: chrono::DateTime<Local> = chrono::DateTime::from(ts);
            let ts = ts - chrono::Days::new(1);
            ts.timestamp()
        } else {
            let ts: chrono::DateTime<Utc> = chrono::DateTime::<Utc>::from_timestamp(timestamp, 0).expect("invalid timestamp");
            let ts: chrono::DateTime<Local> = chrono::DateTime::from(ts);
            let ts = ts - chrono::Months::new(1);
            ts.timestamp()
        };

        let userdata = get_user_info(userid).await?;

        let ts = Self::get_timespan(temporary, prevtimestamp);
		let tst = Self::get_timespan(false, prevtimestamp);

        let invoiceentries = get_invoice(userid, ts.from, ts.to).await?;
        let total_sum = get_user_invoice_sum(userid, tst.from, tst.to).await?;

        /* invoice id */
        let pdffilename = format!("{}_{}_{}.pdf", invoiceid, &userdata.firstname, &userdata.lastname);

        let htmlmsg = self.generate_invoice_message(MessageType::Html, temporary, Self::get_address(&userdata.gender), &userdata.lastname, &invoiceentries, total_sum)?;
        let plainmsg = self.generate_invoice_message(MessageType::Plain, temporary, Self::get_address(&userdata.gender), &userdata.lastname, &invoiceentries, total_sum)?;

        /* pdf generation */
        let pdfdata = if !temporary {
            let dbus_connection = Connection::system().await?;
            let pdf = ShopPDFProxy::new(&dbus_connection).await?;

            pdf.set_invoice_id(invoiceid).await?;
            pdf.set_invoice_date(timestamp).await?;
            pdf.set_invoice_recipient(InvoiceRecipient {
                firstname: userdata.firstname.clone(),
                lastname: userdata.lastname.clone(),
                street: userdata.street.clone(),
                postal_code: userdata.postal_code.clone(),
                city: userdata.city.clone(),
                gender: userdata.gender.clone(),
            }).await?;
            pdf.set_invoice_entries(invoiceentries).await?;
            let pdfdata = pdf.generate().await?;
            pdf.clear().await?;
            pdfdata
        } else {
            Vec::new()
        };

        Ok(InvoiceData {
            html: htmlmsg,
            plain: plainmsg,
            pdfdata: pdfdata,
            pdffilename: pdffilename,
        })
	}

	fn get_treasurer_text(&self) -> Result<String, std::io::Error> {
        let file = format!("{}/{}", self.datadir, "treasurer.mail.txt");
        let text = std::fs::read_to_string(file)?;
        let text = text.replace("{{{SHORTNAME}}}", &self.shortname);

		Ok(text)
	}

	fn get_timespan(temporary: bool, timestamp: i64) -> Timespan {
        let time: chrono::DateTime<Utc> = chrono::DateTime::<Utc>::from_timestamp(timestamp, 0).expect("invalid timestamp");
        let time: chrono::DateTime<Local> = chrono::DateTime::from(time);

        if temporary {
            let start = chrono::NaiveDate::from_ymd_opt(time.year(), time.month(), time.day()).unwrap().and_hms_opt(8, 0, 0).unwrap();
            let mut start: chrono::DateTime<Local> = chrono::Local.from_local_datetime(&start).unwrap();

            /* provided timestamp is from before 8:00 and should be for the previous day */
            if start > time {
                start = start - chrono::Days::new(1);
            }

            let stop = start + chrono::Days::new(1);

            Timespan {
                from: start.timestamp(),
                to: stop.timestamp() - 1,
            }
        } else {
            let start = chrono::NaiveDate::from_ymd_opt(time.year(), time.month(), 1).unwrap().and_hms_opt(0, 0, 0).unwrap();
            let start: chrono::DateTime<Local> = chrono::Local.from_local_datetime(&start).unwrap();
            let stop = start + chrono::Months::new(1);

            Timespan {
                from: start.timestamp(),
                to: stop.timestamp() - 1,
            }
        }
	}

	fn get_address(gender: &str) -> &'static str {
		match gender {
			"masculinum" => "Sehr geehrter Herr",
			"femininum" => "Sehr geehrte Frau",
            _ => "Moin",
		}
	}

	fn generate_invoice_message(&self, msgtype: MessageType, temporary: bool, address: &str, name: &str, entries: &Vec<InvoiceEntry>, total_sum: i32) -> Result<String, std::io::Error> {
        let filename = match (msgtype, temporary) {
            (MessageType::Html, true) => "invoice.temporary.html",
            (MessageType::Plain, true) => "invoice.temporary.txt",
            (MessageType::Html, false) => "invoice.final.html",
            (MessageType::Plain, false) => "invoice.final.txt",
        };
        let filename = format!("{}/{}", self.datadir, filename);

        let vatfile = match msgtype {
            MessageType::Plain => "vat.txt",
            MessageType::Html => "vat.html",
        };
        let vatfile = format!("{}/{}", self.datadir, vatfile);

        let table = match msgtype {
            MessageType::Plain => Self::generate_invoice_table_text(entries),
            MessageType::Html => Self::generate_invoice_table_html(entries),
        };

        let sum_month_str = format!("{},{:02}", total_sum / 100, total_sum % 100);

        let text = std::fs::read_to_string(filename)?;
		let text = text.replace("{{{ADDRESS}}}", &address);
		let text = text.replace("{{{LASTNAME}}}", &name);
		let text = text.replace("{{{SPACENAME}}}", &self.spacename);
		let text = text.replace("{{{INVOICE_TABLE}}}", &table);
		let text = text.replace("{{{SUM_MONTH}}}", &sum_month_str);

        let vatinfotext = std::fs::read_to_string(vatfile)?;
        let text = text.replace("{{{VAT}}}", &vatinfotext);

		Ok(text)
	}

	fn generate_invoice_table_text(entries: &Vec<InvoiceEntry>) -> String {
		let mut result = String::new();

		let article_minsize = "Artikel".graphemes(true).count();

		// no articles bought
		if entries.len() == 0 {
			return result;
        }

		// get length of longest name + invoice sum
		let mut maxnamelength = article_minsize;
		let mut total = 0;
		for entry in entries {
			if maxnamelength < entry.product.name.graphemes(true).count() {
				maxnamelength = entry.product.name.graphemes(true).count();
            }
			total += entry.price;
		}

		// generate table header
        result.push_str(&format!(" +------------+----------+-{}-+----------+\n", "-".repeat(maxnamelength)));
        result.push_str(&format!(" | Datum      | Uhrzeit  | Artikel{} | Preis    |\n", " ".repeat(maxnamelength - article_minsize)));
        result.push_str(&format!(" +------------+----------+-{}-+----------+\n", "-".repeat(maxnamelength)));

		// generate table data
		let mut lastdate = String::new();
		for entry in entries {
            let dt: chrono::DateTime<Utc> = chrono::DateTime::<Utc>::from_timestamp(entry.timestamp, 0).expect("invalid timestamp");
            let dt: chrono::DateTime<Local> = chrono::DateTime::from(dt);
            let newdate = dt.format("%Y-%m-%d").to_string();
            let time = dt.format("%H:%M:%S").to_string();
            let date = if lastdate == newdate { "          ".to_string() } else { lastdate = newdate.clone(); newdate };
            let namelength = entry.product.name.graphemes(true).count();

            result.push_str(&format!(" | {} | {} | {}{} | {:>3},{:02} € |\n", date, time, entry.product.name, " ".repeat(maxnamelength-namelength), entry.price / 100, entry.price % 100));
		}

		// generate table footer
        result.push_str(&format!(" +------------+----------+-{}-+----------+\n", "-".repeat(maxnamelength)));
        result.push_str(&format!(" | Summe:                  {} | {:>3},{:02} € |\n", " ".repeat(maxnamelength), total / 100, total % 100));
        result.push_str(&format!(" +-------------------------{}-+----------+\n", "-".repeat(maxnamelength)));

		result
	}

	fn generate_invoice_table_html(entries: &Vec<InvoiceEntry>) -> String {
        let mut result = String::new();
        let mut lastdate = String::new();
        let mut total = 0;

        result.push_str("<table cellpadding=\"5\" style=\"border-collapse:collapse;\">\n");
        result.push_str("\t<tr>\n");
        result.push_str("\t\t<th style=\"border: 1px solid black;\">Datum</th>\n");
        result.push_str("\t\t<th style=\"border: 1px solid black;\">Zeit</th>\n");
        result.push_str("\t\t<th style=\"border: 1px solid black;\">Artikel</th>\n");
        result.push_str("\t\t<th style=\"border: 1px solid black;\">Preis</th>\n");
        result.push_str("\t</tr>\n");

        for entry in entries {
            let dt: chrono::DateTime<Utc> = chrono::DateTime::<Utc>::from_timestamp(entry.timestamp, 0).expect("invalid timestamp");
            let dt: chrono::DateTime<Local> = chrono::DateTime::from(dt);

            let newdate = dt.format("%Y-%m-%d").to_string();
            let time = dt.format("%H:%M:%S").to_string();
            let date = if lastdate == newdate { String::new() } else { lastdate = newdate.clone(); newdate };

            total += entry.price;

            result.push_str("\t<tr>\n");
            result.push_str(&format!("\t\t<td style=\"border: 1px solid black;\">{}</td>\n", date));
            result.push_str(&format!("\t\t<td style=\"border: 1px solid black;\">{}</td>\n", time));
            result.push_str(&format!("\t\t<td style=\"border: 1px solid black;\">{}</td>\n", entry.product.name));
            result.push_str(&format!("\t\t<td style=\"border: 1px solid black;\" align=\"right\"><tt>{},{:02} €</tt></td>\n", entry.price / 100, entry.price % 100));
            result.push_str("\t</tr>\n");
        }

        result.push_str("\t<tr>\n");
        result.push_str("\t\t<th style=\"border: 1px solid black;\" colspan=\"3\" align=\"left\">Summe:</th>\n");
        result.push_str(&format!("\t\t<td style=\"border: 1px solid black;\" align=\"right\"><tt>{},{:02} €</tt></td>\n", total / 100, total % 100));
        result.push_str("\t</tr>\n");

        result.push_str("</table>\n");
        result
	}
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Cli::parse();
    let mut cfg = Ini::new();
    cfg.load("/etc/shopsystem/config.ini").expect("failed to load config");
    let datapath = cfg.get("GENERAL", "datapath").unwrap_or("/usr/share/shopsystem/".to_string());
    let datapath = format!("{}/invoice", datapath);

    let mailfromaddress = cfg.get("MAIL", "mailfromaddress").expect("config does not specify MAIL mailfromaddress");
    let treasurermailaddress = cfg.get("MAIL", "treasurermailaddress").expect("config does not specify MAIL treasurermailaddress");
    let shortname = cfg.get("GENERAL", "shortname").expect("config does not specify GENERAL shortname");
    let spacename = cfg.get("GENERAL", "spacename").expect("config does not specify GENERAL spacename");
    let jverein_membership_number = cfg.get("JVEREIN", "membership_number").expect("config does not specify JVEREIN membership_number");

    let invoicer = Invoicer {
        datadir: datapath,
        mailfromaddress: mailfromaddress,
        treasurermailaddress: treasurermailaddress,
        shortname: shortname,
        spacename: spacename,
        jverein_membership_number: jverein_membership_number,
    };

    let temporary = args.day;
    let timestamp = args.timestamp.unwrap_or(chrono::Utc::now().timestamp());
    let user = args.user;
    invoicer.send_invoices(temporary, timestamp, user).await?;

    Ok(())
}
