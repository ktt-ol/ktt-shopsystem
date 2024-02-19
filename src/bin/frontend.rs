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

use ratatui::{
    backend::CrosstermBackend,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    layout::{Layout, Constraint, Direction, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Line},
    Terminal,
    Frame
};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use zbus::{self, Connection, proxy};
use async_recursion::async_recursion;

static ZERO: [&str; 3] = [
    " _ ",
    "/ \\",
    "\\_/"
];

static ONE: [&str; 3] = [
    "   ",
    " /|",
    "  |"
];

static TWO: [&str; 3] = [
    "__ ",
    " _)",
    "(__"
];

static THREE: [&str; 3] = [
    "__ ",
    " _)",
    "__)"
];

static FOUR: [&str; 3] = [
    "   ",
    "|_|",
    "  |"
];

static FIVE: [&str; 3] = [
    " __",
    "|_ ",
    "__)"
];

static SIX: [&str; 3] = [
    " _ ",
    "/_ ",
    "\\_)"
];

static SEVEN: [&str; 3] = [
    "___",
    "  /",
    " / "
];

static EIGHT: [&str; 3] = [
    " _ ",
    "(_)",
    "(_)"
];

static NINE: [&str; 3] = [
    " _ ",
    "(_\\",
    " _/"
];

static COLON: [&str; 3] = [
		"   ",
		" o ",
		" o "
];

static SPACE: [&str; 3] = [
		"   ",
		"   ",
		"   "
];

fn ascii_number(c: char) -> [&'static str; 3] {
    match c {
        '0' => ZERO,
        '1' => ONE,
        '2' => TWO,
        '3' => THREE,
        '4' => FOUR,
        '5' => FIVE,
        '6' => SIX,
        '7' => SEVEN,
        '8' => EIGHT,
        '9' => NINE,
        ':' => COLON,
        _ => SPACE,
    }
}

#[derive(PartialEq)]
enum LogType {
    DateChange,
    Info,
    Error,
    Warning,
}

struct LogEntry {
    time: chrono::DateTime<chrono::Local>,
    logtype: LogType,
    msg: String,
}

fn asciify(time: &str) -> [String; 3] {
    let mut line1 = " ".to_string();
    let mut line2 = " ".to_string();
    let mut line3 = " ".to_string();
    for c in time.chars() {
        let c = ascii_number(c);
        line1.push_str(c[0]);
        line2.push_str(c[1]);
        line3.push_str(c[2]);
    }
    [line1, line2, line3]
}

fn clock(f: &mut Frame, draw_dots: bool, area: Rect) {
    let now = chrono::Local::now();
    let date = now.format("      %d.%m.%Y").to_string();
    let time = if draw_dots {
        now.format("%H:%M").to_string()
    } else {
        now.format("%H %M").to_string()
    };
    let time = asciify(&time);

    let text = vec![
        Line::from(Span::raw(date)),
        Line::from(Span::raw("")),
        Line::from(Span::styled(&time[0], Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled(&time[1], Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled(&time[2], Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))),
    ];
    let p = Paragraph::new(text).style(Style::default().fg(Color::White));
    f.render_widget(p, area);
}

fn logo(f: &mut Frame, area: Rect) {
    let logo = std::fs::read_to_string("/etc/shopsystem/logo.txt").unwrap();
    let logo: Vec<&str> = logo.split("\n").collect();
    let mut text = Vec::new();

    for line in logo {
        text.push(Line::from(Span::styled(line, Style::default().fg(Color::Green))));
    }

    let p = Paragraph::new(text).style(Style::default().fg(Color::White));
    f.render_widget(p, area);
}

fn log(f: &mut Frame, area: Rect, logdata: &Vec<LogEntry>) {
    let linewidth = (area.width as usize) - 2;
    let timestampwidth = "[00:00:00] ".len();
    let wraplen = linewidth - timestampwidth;
    let items: Vec<ListItem> = logdata.iter().map(|logentry| {
        let lines = if logentry.logtype == LogType::DateChange {
            vec![Line::from(Span::raw(format!("--- Date change {} ---", logentry.time.format("%d.%m.%Y"))))]
        } else {
            let wrappedmsg = textwrap::wrap(&logentry.msg, wraplen);
            wrappedmsg.iter().enumerate().map(|(i,line)| {
                if i == 0 {
                    Line::from(Span::raw(format!("[{}] {}", logentry.time.format("%H:%M:%S"), line)))
                } else {
                    Line::from(Span::raw(format!("           {}", line)))
                }
            }).collect()
        };
        let style = match logentry.logtype {
            LogType::Info => Style::default(),
            LogType::Error => Style::default().fg(Color::Red),
            LogType::Warning => Style::default().fg(Color::Yellow),
            LogType::DateChange => Style::default(),
        };
        ListItem::new(lines).style(style)
    }).collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Log"));

    let selection = if logdata.len() > 0 {
        Some(logdata.len() - 1)
    } else {
        None
    };

    let mut state = ListState::default();
    state.select(selection);
    f.render_stateful_widget(list, area, &mut state);
}

fn ui(f: &mut Frame, draw_dots: bool, logdata: &Vec<LogEntry>) {
    let vsplit = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .constraints(
            [
                Constraint::Length(6),
                Constraint::Min(5),
            ].as_ref()
        )
        .split(f.size());
    let hsplit = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Min(20),
                Constraint::Length(17),
            ].as_ref()
        )
        .split(vsplit[0]);
    logo(f, hsplit[0]);
    clock(f, draw_dots, hsplit[1]);
    log(f, vsplit[1], logdata);
}

fn thread_input(sender: tokio::sync::mpsc::Sender<String>, runtime: tokio::runtime::Handle) {
    std::thread::spawn(move || {
        let mut buffer = String::new();
        loop {
            match event::read().unwrap() {
                Event::Resize(_w, _h) => {
                            let sender2 = sender.clone();
                            runtime.spawn(async move {
                                sender2.send(format!("resize")).await.expect("Communication Error");
                            });
                },
                Event::Key(key) => {
                    match key.code {
                        KeyCode::Enter => {
                            let sender2 = sender.clone();
                            let line = buffer.clone();
                            runtime.spawn(async move {
                                sender2.send(line).await.expect("Communication Error");
                            });
                            buffer.clear();
                        }
                        KeyCode::Char(c) => {
                            buffer.push(c);
                        }
                        KeyCode::Backspace => {
                            buffer.pop();
                        }
                        KeyCode::Esc => {
                            let sender2 = sender.clone();
                            runtime.spawn(async move {
                                sender2.send("exit".to_string()).await.expect("Communication Error");
                            });
                        }
                        _ => {},
                    }
                },
                _ => {}
            }
        }
    });
}

fn thread_timer(timer_sender: tokio::sync::watch::Sender<bool>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            timer_sender.send(true).expect("Communication Error");
        }
    });
}

fn code39_gen_checksum(text: &str) -> Option<char> {
    let mut sum = 0;

    for c in text.chars() {
        match c {
            '*' => { sum += 0; },
            '0' => { sum += 0; },
            '1' => { sum += 1; },
            '2' => { sum += 2; },
            '3' => { sum += 3; },
            '4' => { sum += 4; },
            '5' => { sum += 5; },
            '6' => { sum += 6; },
            '7' => { sum += 7; },
            '8' => { sum += 8; },
            '9' => { sum += 9; },
            'A' => { sum += 10; },
            'B' => { sum += 11; },
            'C' => { sum += 12; },
            'D' => { sum += 13; },
            'E' => { sum += 14; },
            'F' => { sum += 15; },
            'G' => { sum += 16; },
            'H' => { sum += 17; },
            'I' => { sum += 18; },
            'J' => { sum += 19; },
            'K' => { sum += 20; },
            'L' => { sum += 21; },
            'M' => { sum += 22; },
            'N' => { sum += 23; },
            'O' => { sum += 24; },
            'P' => { sum += 25; },
            'Q' => { sum += 26; },
            'R' => { sum += 27; },
            'S' => { sum += 28; },
            'T' => { sum += 29; },
            'U' => { sum += 30; },
            'V' => { sum += 31; },
            'W' => { sum += 32; },
            'X' => { sum += 33; },
            'Y' => { sum += 34; },
            'Z' => { sum += 35; },
            '-' => { sum += 36; },
            '.' => { sum += 37; },
            ' ' => { sum += 38; },
            '$' => { sum += 39; },
            '/' => { sum += 40; },
            '+' => { sum += 41; },
            '%' => { sum += 42; },
            _ => { return None },
        }
    }

    let result = match sum % 43 {
         0 => '0',
         1 => '1',
         2 => '2',
         3 => '3',
         4 => '4',
         5 => '5',
         6 => '6',
         7 => '7',
         8 => '8',
         9 => '9',
        10 => 'A',
        11 => 'B',
        12 => 'C',
        13 => 'D',
        14 => 'E',
        15 => 'F',
        16 => 'G',
        17 => 'H',
        18 => 'I',
        19 => 'J',
        20 => 'K',
        21 => 'L',
        22 => 'M',
        23 => 'N',
        24 => 'O',
        25 => 'P',
        26 => 'Q',
        27 => 'R',
        28 => 'S',
        29 => 'T',
        30 => 'U',
        31 => 'V',
        32 => 'W',
        33 => 'X',
        34 => 'Y',
        35 => 'Z',
        36 => '-',
        37 => '.',
        38 => ' ',
        39 => '$',
        40 => '/',
        41 => '+',
        42 => '%',
         _ => '!', // cannot happen with % 43
    };

    Some(result)
}

fn check_valid_gtin(ean: u64, length: usize) -> bool {
    if format!("{}", ean).len() > length {
        return false;
    }

    let mut counter = 1;
    let mut tmp = ean;
    let mut checksum = 0;
    while counter < length {
        let weight = if counter % 2 == 1 { 3 } else { 1 };
        tmp = tmp / 10;
        checksum += (tmp % 10) * weight;
        counter += 1;
    }
    checksum = 10 - (checksum % 10);
    checksum = if checksum == 10 { 0 } else { checksum };

    ean % 10 == checksum
}

/// true if this is Code39 with valid checksum
fn code39_check(line: &str) -> bool {
    if line.len() == 0 {
        return false;
    }

    let checksum = line.chars().last().unwrap();
    let text = &line[0..line.len()-1];
    let calculated_checksum = code39_gen_checksum(text);

    if calculated_checksum.is_none() {
        return false;
    }

    calculated_checksum.unwrap() == checksum
}

#[derive(PartialEq)]
enum ShopInstruction {
    Invalid,
    InvalidCode39Checksum,
    BrokenUserID,
    Login,
    Logout,
    Revert,
    EAN,
    RFID,
}

struct ShopCommand {
    instruction: ShopInstruction,
    userid: Option<i32>,
    productid: Option<u64>,
    rfiddata: Option<String>,
}

impl ShopCommand {
    fn parse(line: &str) -> Self {
        let is_code39 = code39_check(line);
        let mut ean: Option<u64> = line.parse().ok();

        if ean.is_some() {
            if !check_valid_gtin(ean.unwrap(), 8) && !check_valid_gtin(ean.unwrap(), 13) {
                ean = None;
            }
        }

        if line.starts_with("USER ") {
            if !is_code39 {
                ShopCommand { instruction: ShopInstruction::InvalidCode39Checksum, userid: None, productid: None, rfiddata: None }
            } else {
                let userid: Option<i32> = line[5..line.len()-1].parse().ok();
                if userid.is_none() {
                    ShopCommand { instruction: ShopInstruction::BrokenUserID, userid: None, productid: None, rfiddata: None }
                } else {
                    ShopCommand { instruction: ShopInstruction::Login, userid: userid, productid: None, rfiddata: None }
                }
            }
        } else if line == "GUEST" {
            ShopCommand { instruction: ShopInstruction::Login, userid: Some(0), productid: None, rfiddata: None }
        } else if line == "LOGOUT" {
            ShopCommand { instruction: ShopInstruction::Logout, userid: None, productid: None, rfiddata: None }
        } else if line == "UNDO" {
            ShopCommand { instruction: ShopInstruction::Revert, userid: None, productid: None, rfiddata: None }
        } else if ean.is_some() {
            ShopCommand { instruction: ShopInstruction::EAN, userid: None, productid: ean, rfiddata: None }
        } else if line.len() == 10 {
            ShopCommand { instruction: ShopInstruction::RFID, userid: None, productid: None, rfiddata: Some(line.to_string()) }
        } else {
            ShopCommand { instruction: ShopInstruction::Invalid, userid: None, productid: None, rfiddata: None }
        }
    }
}

#[proxy(
    interface = "io.mainframe.shopsystem.AudioPlayer",
    default_service = "io.mainframe.shopsystem.AudioPlayer",
    default_path = "/io/mainframe/shopsystem/audio"
)]
trait ShopAudio {
    async fn get_random_user_theme(&self) -> zbus::Result<String>;
    async fn play_user(&self, theme: &str, name: &str) -> zbus::Result<()>;
    async fn play_system(&self, file: &str) -> zbus::Result<()>;
}

async fn play_user(theme: &str, name: &str) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let proxy = ShopAudioProxy::new(&connection).await?;
    proxy.play_user(theme, name).await
}

async fn play_system(file: &str) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let proxy = ShopAudioProxy::new(&connection).await?;
    proxy.play_system(file).await
}

#[proxy(
    interface = "io.mainframe.shopsystem.Database",
    default_service = "io.mainframe.shopsystem.Database",
    default_path = "/io/mainframe/shopsystem/database"
)]
trait ShopDB {
    async fn get_userid_for_rfid(&self, rfid: &str) -> zbus::Result<i32>;
    async fn get_username(&self, userid: i32) -> zbus::Result<String>;
    async fn get_user_theme(&self, user: i32, fallback: String) -> zbus::Result<String>;

    async fn ean_alias_get(&self, ean: u64) -> zbus::Result<u64>;
    async fn get_product_name(&self, ean: u64) -> zbus::Result<String>;
    async fn get_product_price(&self, user: i32, article: u64) -> zbus::Result<i32>;

	async fn buy(&self, user: i32, article: u64) -> zbus::Result<()>;
}

async fn get_username(uid: i32) -> zbus::Result<String> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_username(uid).await
}

async fn get_user_theme(uid: i32) -> zbus::Result<String> {
    let connection = Connection::system().await?;
    let audio = ShopAudioProxy::new(&connection).await?;
    let db = ShopDBProxy::new(&connection).await?;
    let fallback = audio.get_random_user_theme().await.unwrap_or("".to_string());
    db.get_user_theme(uid, fallback).await
}

async fn get_userid_for_rfid(rfid: &str) -> Option<i32> {
    let connection = Connection::system().await.ok()?;
    let proxy = ShopDBProxy::new(&connection).await.ok()?;
    proxy.get_userid_for_rfid(rfid).await.ok()
}

struct Product {
    ean: u64,
    name: String,
    price: i32,
    guest_price: i32,
}

async fn get_product_info(ean: u64) -> zbus::Result<Product> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    let ean = proxy.ean_alias_get(ean).await?;

    Ok(Product {
        ean: ean,
        name: proxy.get_product_name(ean).await?,
        price: proxy.get_product_price(1, ean).await?,
        guest_price: proxy.get_product_price(0, ean).await?,
    })
}

async fn get_product_info_for_user(ean: u64, user: i32) -> zbus::Result<Product> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    let ean = proxy.ean_alias_get(ean).await?;

    Ok(Product {
        ean: ean,
        name: proxy.get_product_name(ean).await?,
        price: proxy.get_product_price(user, ean).await?,
        guest_price: 0,
    })
}

async fn buy(user: i32, article: u64) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.buy(user, article).await
}

struct ShopState {
    /// TUI log
    logdata: Vec<LogEntry>,
    /// Some(user ID of logged in user) or None if there is no session
    user: Option<i32>,
    /// Audio theme
    audiotheme: Option<String>,
    /// List of product ids user currently has in his shopping cart
    cart: Vec<Product>,
}

fn price2str(price: i32) -> String {
    format!("{}.{:02}€", price/100, price%100)
}

impl ShopState {
    #[async_recursion]
    async fn execute(&mut self, cmd: ShopCommand) {
        let time = chrono::Local::now();

        if self.user.is_some() {
            let userid = self.user.unwrap();

            match cmd.instruction {
                ShopInstruction::Invalid => {
                    self.logdata.push(LogEntry{time: time, logtype: LogType::Error, msg: "Invalid command".to_string()});
                    let _ = play_user(&self.audiotheme.as_ref().unwrap(), "error").await;
                },
                ShopInstruction::InvalidCode39Checksum => {
                    self.logdata.push(LogEntry{time: time, logtype: LogType::Error, msg: "Code39 checksum invalid".to_string()});
                    let _ = play_system("error.ogg").await;
                },
                ShopInstruction::BrokenUserID => {
                    self.logdata.push(LogEntry{time: time, logtype: LogType::Error, msg: "Missing or invalid user ID".to_string()});
                    let _ = play_system("error.ogg").await;
                },
                ShopInstruction::Login => {
                    self.execute( ShopCommand { instruction: ShopInstruction::Logout, userid: None, productid: None, rfiddata: None } ).await;
                    self.execute( ShopCommand { instruction: ShopInstruction::Login, userid: cmd.userid, productid: None, rfiddata: None } ).await;
                },
                ShopInstruction::Logout => {
                    let mut sum = 0;
                    for product in &self.cart {
                        sum += product.price;
                        match buy(userid, product.ean).await {
                            Ok(_) => {},
                            Err(err) => {
                                self.logdata.push(LogEntry{time: time, logtype: LogType::Info, msg: format!("Error: {}", err)});
                                let _ = play_user(&self.audiotheme.as_ref().unwrap(), "error").await;
                            }
                        }
                    }
                    
                    if userid >= 0 {
                        self.logdata.push(LogEntry{time: time, logtype: LogType::Info, msg: format!("Logout, bought {} articles for {}", self.cart.len(), price2str(sum))});
                    } else {
                        self.logdata.push(LogEntry{time: time, logtype: LogType::Info, msg: format!("Logout, bought {} articles", self.cart.len())});
                    }
                    let _ = play_user(&self.audiotheme.as_ref().unwrap(), "logout").await;
                    self.user = None;
                    self.cart.clear();
                },
                ShopInstruction::Revert => {
                    if self.cart.is_empty() {
                        self.logdata.push(LogEntry{time: time, logtype: LogType::Error, msg: "Cart is empty".to_string()});
                        let _ = play_user(&self.audiotheme.as_ref().unwrap(), "error").await;
                    } else {
                        let product = self.cart.pop().unwrap();
                        self.logdata.push(LogEntry{time: time, logtype: LogType::Error, msg: format!("Undo {}", product.name)});
                        let _ = play_user(&self.audiotheme.as_ref().unwrap(), "purchase").await;
                    }
                },
                ShopInstruction::EAN => {
                    let productid = cmd.productid.unwrap();
                    let product = get_product_info_for_user(productid, userid).await;
                    match product {
                        Ok(product) => {
                            if userid >= 0 {
                                self.logdata.push(LogEntry{time: time, logtype: LogType::Info, msg: format!("Buy: {} - {}", product.name, price2str(product.price))});
                            } else {
                                self.logdata.push(LogEntry{time: time, logtype: LogType::Info, msg: format!("Buy: {}", product.name)});
                            }
                            self.cart.push(product);
                            let _ = play_user(&self.audiotheme.as_ref().unwrap(), "purchase").await;
                        },
                        Err(_error) => {
                            self.logdata.push(LogEntry{time: time, logtype: LogType::Error, msg: format!("Unknown product: {productid}")});
                            let _ = play_user(&self.audiotheme.as_ref().unwrap(), "error").await;
                        }
                    }
                },
                ShopInstruction::RFID => {
                    self.execute( ShopCommand { instruction: ShopInstruction::Logout, userid: None, productid: None, rfiddata: None } ).await;
                    self.execute( ShopCommand { instruction: ShopInstruction::RFID, userid: None, productid: None, rfiddata: cmd.rfiddata } ).await;
                },
            }
        } else {
            match cmd.instruction {
                ShopInstruction::Invalid => {
                    self.logdata.push(LogEntry{time: time, logtype: LogType::Error, msg: "Invalid command".to_string()});
                    let _ = play_system("error.ogg").await;
                },
                ShopInstruction::InvalidCode39Checksum => {
                    self.logdata.push(LogEntry{time: time, logtype: LogType::Error, msg: "Code39 checksum invalid".to_string()});
                    let _ = play_system("error.ogg").await;
                },
                ShopInstruction::BrokenUserID => {
                    self.logdata.push(LogEntry{time: time, logtype: LogType::Error, msg: "Missing or invalid user ID".to_string()});
                    let _ = play_system("error.ogg").await;
                },
                ShopInstruction::Login => {
                    let userid = cmd.userid.unwrap();
                    let username = get_username(userid).await;
                    let audiotheme = get_user_theme(userid).await;
                    match username {
                        Ok(username) => {
                            let username = username.trim();
                            self.logdata.push(LogEntry{time: time, logtype: LogType::Info, msg: format!("Login as {username} ({userid})")});
                            self.user = Some(userid);
                            self.audiotheme = audiotheme.ok();
                            let _ = play_user(&self.audiotheme.as_ref().unwrap(), "login").await;
                        },
                        Err(error) => {
                            self.logdata.push(LogEntry{time: time, logtype: LogType::Error, msg: format!("No such user: {}", error.to_string())});
                            let _ = play_system("error.ogg").await;
                        }
                    }
                },
                ShopInstruction::Logout => {
                    self.logdata.push(LogEntry{time: time, logtype: LogType::Error, msg: "No active session".to_string()});
                    let _ = play_system("error.ogg").await;
                },
                ShopInstruction::Revert => {
                    self.logdata.push(LogEntry{time: time, logtype: LogType::Error, msg: "No active session".to_string()});
                    let _ = play_system("error.ogg").await;
                },
                ShopInstruction::EAN => {
                    let productid = cmd.productid.unwrap();
                    let product = get_product_info(productid).await;
                    match product {
                        Ok(product) => {
                            self.logdata.push(LogEntry{time: time, logtype: LogType::Warning, msg: format!("Price Info: {} - Member {} Guest {}", product.name, price2str(product.price), price2str(product.guest_price))});
                            let _ = play_system("error.ogg").await;
                        },
                        Err(_error) => {
                            self.logdata.push(LogEntry{time: time, logtype: LogType::Error, msg: format!("Unknown product: {}", productid)});
                            let _ = play_system("error.ogg").await;
                        }
                    }
                },
                ShopInstruction::RFID => {
                    let rfid = cmd.rfiddata.unwrap();
                    let userid = get_userid_for_rfid(&rfid).await;
                    
                    match userid {
                        None => {
                            self.logdata.push(LogEntry{time: time, logtype: LogType::Error, msg: format!("Unknown RFID token")});
                            let _ = play_system("error.ogg").await;
                        },
                        Some(userid) => {
                            self.execute(ShopCommand { instruction: ShopInstruction::Login, userid: Some(userid), productid: None, rfiddata: None } ).await;
                        }
                    }
                },
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = std::io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (input_sender, mut input_receiver) = tokio::sync::mpsc::channel(1);
    thread_input(input_sender, tokio::runtime::Handle::current());

    let (timer_sender, mut timer_receiver) = tokio::sync::watch::channel(false);
    thread_timer(timer_sender);

    let mut state = ShopState { logdata: Vec::new(), user: None, audiotheme: None, cart: Vec::new() };
    let mut draw_dots = true;
    let mut last_date = chrono::Local::now();

    let _ = play_system("startup.ogg").await;

    state.logdata.push(LogEntry{time: chrono::Local::now(), logtype: LogType::Info, msg: "System started up".to_string()});

    loop {
        terminal.draw(|f| ui(f, draw_dots, &state.logdata))?;
        tokio::select! {
            Some(line) = input_receiver.recv() => {
                match line.as_str() {
                    "exit"|"quit" => { break; },
                    "resize" => { continue; },
                    "clear log" => { state.logdata.clear(); },
                    _ => {
                        state.execute(ShopCommand::parse(&line)).await;
                    },
                }
            }
            Ok(_) = timer_receiver.changed() => {
                let now = chrono::Local::now();
                if last_date.date_naive() != now.date_naive() {
                    last_date = now;
                    state.logdata.push(LogEntry{time: now, logtype: LogType::DateChange, msg: "".to_string()});
                }
                draw_dots = !draw_dots;
            },
        }
    }

    let _ = play_system("shutdown.ogg").await;

    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;
    disable_raw_mode()?;

    Ok(())
}
