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

#[macro_use] extern crate rocket;
use rocket_dyn_templates::{Template, context};
use rocket::serde::json::Json;
use rocket::fs::TempFile;
use rocket::response::status::Forbidden;
use serde::{Deserialize, Serialize};
use rocket::form::Form;
use rocket::data::Capped;
use rocket::http::{Cookie, CookieJar};
use std::{collections::HashMap, hash::BuildHasher};
use zbus;
use zbus::{Connection, proxy, zvariant::Type};
use rand::{RngExt, distr::Alphanumeric};
use chrono;
use chrono::prelude::*;
use std::num::ParseIntError;
use rocket::data::{Data, ToByteUnit};
use rocket::response::Responder;
use configparser::ini::Ini;
use barcoders::sym::code39::*;
use barcoders::generators::svg::*;
use rocket::http::ContentType;
use pangocairo::glib::Bytes;

enum WebShopError {
    IOError(std::io::Error),
    DBusError(zbus::Error),
    PermissionDenied(),
    UserInfoListError(UserInfoListError),
    BarcodeError(barcoders::error::Error),
    CairoError(cairo::Error),
    RSVGLoadingError(rsvg::LoadingError),
    RSVGRenderingError(rsvg::RenderingError),
    UTF8Error(std::string::FromUtf8Error),
    UnboxError(std::string::String),
}

impl From<zbus::Error> for WebShopError {
    fn from(err: zbus::Error) -> WebShopError {
            WebShopError::DBusError(err)
    }
}

impl From<std::io::Error> for WebShopError {
    fn from(err: std::io::Error) -> WebShopError {
            WebShopError::IOError(err)
    }
}

impl From<UserInfoListError> for WebShopError {
    fn from(err: UserInfoListError) -> WebShopError {
            WebShopError::UserInfoListError(err)
    }
}

impl From<barcoders::error::Error> for WebShopError {
    fn from(err: barcoders::error::Error) -> WebShopError {
            WebShopError::BarcodeError(err)
    }
}

impl From<cairo::Error> for WebShopError {
    fn from(err: cairo::Error) -> WebShopError {
            WebShopError::CairoError(err)
    }
}

impl From<rsvg::LoadingError> for WebShopError {
    fn from(err: rsvg::LoadingError) -> WebShopError {
            WebShopError::RSVGLoadingError(err)
    }
}

impl From<rsvg::RenderingError> for WebShopError {
    fn from(err: rsvg::RenderingError) -> WebShopError {
            WebShopError::RSVGRenderingError(err)
    }
}

impl From<std::string::FromUtf8Error> for WebShopError {
    fn from(err: std::string::FromUtf8Error) -> WebShopError {
            WebShopError::UTF8Error(err)
    }
}

impl<'r> Responder<'r, 'r> for WebShopError {
    fn respond_to(self, req: &rocket::Request) -> rocket::response::Result<'r> {
        match self {
            WebShopError::DBusError(e) => Template::render("error", context! { page: "error", errmsg: e.to_string() }).respond_to(req),
            WebShopError::PermissionDenied() => Template::render("error", context! { page: "error", errmsg: "Permission Denied" }).respond_to(req),
            WebShopError::IOError(e) => Template::render("error", context! { page: "error", errmsg: e.to_string() }).respond_to(req),
            WebShopError::UserInfoListError(e) => Template::render("error", context! { page: "error", errmsg: e.to_string() }).respond_to(req),
            WebShopError::BarcodeError(e) => Template::render("error", context! { page: "error", errmsg: e.to_string() }).respond_to(req),
            WebShopError::CairoError(e) => Template::render("error", context! { page: "error", errmsg: e.to_string() }).respond_to(req),
            WebShopError::RSVGLoadingError(e) => Template::render("error", context! { page: "error", errmsg: e.to_string() }).respond_to(req),
            WebShopError::RSVGRenderingError(e) => Template::render("error", context! { page: "error", errmsg: e.to_string() }).respond_to(req),
            WebShopError::UTF8Error(e) => Template::render("error", context! { page: "error", errmsg: e.to_string() }).respond_to(req),
            WebShopError::UnboxError(e) => Template::render("error", context! { page: "error", errmsg: e.to_string() }).respond_to(req),
        }
    }
}

#[allow(dead_code)]
enum UserInfoListError {
    IO(std::io::Error),
    ParseInt(std::num::ParseIntError),
    CSV(csv::Error),
}

impl From<std::num::ParseIntError> for UserInfoListError {
    fn from(err: std::num::ParseIntError) -> UserInfoListError {
            UserInfoListError::ParseInt(err)
    }
}

impl From<std::io::Error> for UserInfoListError {
    fn from(err: std::io::Error) -> UserInfoListError {
            UserInfoListError::IO(err)
    }
}

impl From<csv::Error> for UserInfoListError {
    fn from(err: csv::Error) -> UserInfoListError {
            UserInfoListError::CSV(err)
    }
}

impl std::fmt::Display for UserInfoListError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            UserInfoListError::IO(_) => write!(f, "IO error"),
            UserInfoListError::ParseInt(_) => write!(f, "Integer Parsing error"),
            UserInfoListError::CSV(_) => write!(f, "CSV error"),
        }
    }
}

#[derive(FromForm)]
struct UserLoginData<'r> {
    userid: i32,
    password: &'r str,
}

#[derive(Deserialize,Serialize)]
struct UserChange {
    old: Option<UserInfo>,
    new: Option<UserInfo>,
}

#[derive(FromForm)]
struct FileUpload<'r> {
    file: Capped<TempFile<'r>>,
}

#[derive(Serialize)]
struct UserInfoList {
    data: Vec<UserInfo>,
}

#[derive(Serialize)]
struct DropdownEntry {
    id: usize,
    name: String,
    disabled: bool,
}

impl UserInfoList {
    fn from_csv(data: &str) -> Result<Self, UserInfoListError> {
        let mut csv = csv::ReaderBuilder::new()
            .delimiter(b';')
            .has_headers(false)
            .from_reader(data.as_bytes());
        let mut list = Vec::new();

        for line in csv.records() {
            let line = line?;
            let elements = line.len();
            if elements < 12 {
                println!("short line in CSV");
                continue;
            }
            if line[0].eq("EXTERNEMITGLIEDSNUMMER") {
                continue;
            }

            let mut info = UserInfo {
                id: line[0].parse()?,
                email: line[1].to_string(),
                firstname: line[2].to_string(),
                lastname: line[3].to_string(),
                street: line[4].to_string(),
                postal_code: line[5].to_string(),
                city: line[6].to_string(),
                gender: line[7].to_string(),
                joined_at: line[8].parse()?,
                pgp: line[9].to_string(),
                hidden: (line[10].parse::<i32>()? != 0),
                disabled: (line[11].parse::<i32>()? != 0),
                sound_theme: "".to_string(),
                rfid: Vec::new(),
            };

            info.gender = match info.gender.as_str() {
                "m" => "masculinum".to_string(),
                "w" => "femininum".to_string(),
                _ => "unknown".to_string(),
            };

            for i in 12..elements {
                if line[i].eq("") {
                    continue;
                }

                info.rfid.push(line[i].to_string());
            }

            list.push(info);
        }

        Ok(UserInfoList {
            data: list,
        })
    }
}

#[derive(Deserialize, Serialize)]
struct SystemUser {
    uid: i32,
    name: String,
}

#[derive(Deserialize, Serialize)]
struct Session {
    uid: i32,
    name: String,
    superuser: bool,
    auth_cashbox: bool,
    auth_products: bool,
    auth_users: bool,
}

#[derive(Type, Deserialize, Serialize)]
pub struct StockItem {
    ean: i64,
    name: String,
    category: String,
    amount: i32,
    memberprice: i32,
    guestprice: i32,
}

#[derive(Type, Deserialize, Serialize)]
pub struct ProductInfo {
	ean: i64,
	name: String,
}

#[derive(Type, Deserialize, Serialize)]
pub struct DetailedProductInfo {
	ean: i64,
    aliases: Vec<i64>,
	name: String,
	category: String,
	amount: i32,
	memberprice: i32,
	guestprice: i32,
    deprecated: bool,
}

#[derive(Type, Clone, Copy, Deserialize, Serialize)]
pub struct ProductDiff {
    ean: i64,
    diff: i32,
}

#[derive(Type, Deserialize, Serialize)]
pub struct InventoryData {
    supplier: i32,
    user: i32,
    operations: Vec<ProductDiff>
}

#[derive(Type, Clone, Copy, Deserialize, Serialize)]
pub struct PriceInfo {
    timestamp: i64,
    memberprice: i32,
    guestprice: i32,
}

#[derive(Type, Clone, Copy, Deserialize, Serialize)]
pub enum CashboxUpdateType {
    Loss,
    Withdrawal,
    Donation,
    Deposit,
}

#[derive(Type, Clone, Copy, Deserialize, Serialize)]
pub struct CashboxUpdate {
    update_type: CashboxUpdateType,
    amount: i32,
}

#[derive(Type, Deserialize, Serialize)]
pub struct ProductMetadata {
    product_size: u32,
    product_size_is_weight: bool,
    container_size: u32,
    calories: u32,
    carbohydrates: u32,
    fats: u32,
    proteins: u32,
    deposit: u32,
    container_deposit: u32,
}

impl Default for ProductMetadata {
    fn default() -> Self {
        Self {
            product_size: 0,
            product_size_is_weight: true,
            container_size: 0,
            calories: 0,
            carbohydrates: 0,
            fats: 0,
            proteins: 0,
            deposit: 0,
            container_deposit: 0,
        }
    }
}

#[derive(Type, Deserialize, Serialize)]
pub struct RestockEntry {
	timestamp: i64,
	amount: u32,
	price: u32,
	supplier: i32,
	best_before_date: i64,
}

#[derive(Type, Deserialize, Serialize)]
pub struct RestockEntryNamedSupplier {
	timestamp: i64,
	amount: u32,
	price: u32,
	supplier: String,
	best_before_date: i64,
}

#[derive(Type, Deserialize, Serialize)]
pub struct BestBeforeEntry {
    ean: i64,
    name: String,
    amount: i32,
    best_before_date: i64,
}

#[derive(Type, Deserialize, Serialize)]
pub struct Supplier {
	id: i64,
	name: String,
	postal_code: String,
	city: String,
	street: String,
	phone: String,
	website: String,
}

#[derive(Type, Deserialize, Serialize)]
pub struct EanAlias {
	ean: i64,
	real_ean: i64,
}

#[derive(Type, Deserialize, Serialize)]
pub struct EanAliasWithName {
	ean: i64,
	real_ean: i64,
    name: String,
}

#[derive(FromForm)]
pub struct NewProduct {
	id: i64,
    name: String,
	category: i32,
    memberprice: String,
    guestprice: String,
}

#[derive(FromForm, Type, Deserialize, Serialize)]
pub struct NewSupplier {
    name: String,
    postal_code: String,
    city: String,
    street: String,
    phone: String,
    website: String,
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

#[derive(Type, Clone, Copy, Deserialize, Serialize)]
pub struct UserAuth {
	id: i32,
    superuser: bool,
    auth_cashbox: bool,
    auth_products: bool,
    auth_users: bool,
}

#[derive(Type, Deserialize, Serialize)]
pub struct Product {
	ean: i64,
	name: String,
}

#[derive(Type, Deserialize, Serialize)]
struct UserBasicInfo {
	id: i32,
	firstname: String,
	lastname: String,
}

#[derive(Type, Deserialize, Serialize)]
pub struct InvoiceEntry {
	timestamp: i64,
	product: Product,
	price: i32,
}

#[derive(Type, Deserialize, Serialize)]
pub struct SalesEntry {
	timestamp: i64,
    user: UserBasicInfo,
	product: Product,
}

#[derive(Type, Deserialize, Serialize)]
pub struct UserSaleStatsEntry {
    timedatecode: String,
    count: i32,
}

#[derive(Type, Deserialize, Serialize)]
pub struct CashboxDiff {
	user: i32,
	amount: i32,
	timestamp: i64,
}

#[derive(Deserialize, Serialize)]
pub struct NamedCashboxDiff {
    username: String,
	amount: i32,
	timestamp: i64,
}

#[derive(Type, Deserialize, Serialize)]
pub struct ProductCategory {
    id: i32,
    name: String,
}

#[derive(Type, Deserialize, Serialize)]
pub struct ProductDetails {
    ean: i64,
    name: String,
    category: String,
    amount: i32,
    deprecated: bool,
}

#[proxy(
    interface = "io.mainframe.shopsystem.Database",
    default_service = "io.mainframe.shopsystem.Database",
    default_path = "/io/mainframe/shopsystem/database"
)]
trait ShopDB {
    async fn set_sessionid(&self, userid: i32, sessionid: &str) -> zbus::Result<()>;
    async fn get_user_by_sessionid(&self, sessionid: &str) -> zbus::Result<i32>;
    async fn get_stock(&self) -> zbus::Result<Vec<StockItem>>;
    async fn get_productlist(&self) -> zbus::Result<Vec<DetailedProductInfo>>;
    async fn restock(&self, user: i32, product: i64, amount: u32, price: u32, supplier: i32, best_before_date: i64) -> zbus::Result<()>;
    async fn buy(&self, user: i32, product: i64) -> zbus::Result<()>;
    async fn new_price(&self, product: i64, timestamp: i64, memberprice: i32, guestprice: i32) ->  zbus::Result<()>;
    async fn get_prices(&self, ean: i64) -> zbus::Result<Vec<PriceInfo>>;
    async fn get_product_aliases(&self, ean: i64) -> zbus::Result<Vec<i64>>;
    async fn get_product_name(&self, ean: i64) -> zbus::Result<String>;
    async fn get_product_amount(&self, ean: i64) -> zbus::Result<i32>;
    async fn get_product_amount_with_container_size(&self, ean: i64) -> zbus::Result<(i32, u32)>;
	async fn get_product_sales_info(&self, ean: i64, since: i64) -> zbus::Result<u32>;
    async fn get_product_category(&self, ean: i64) -> zbus::Result<String>;
    async fn get_product_deprecated(&self, ean: i64) -> zbus::Result<bool>;
    async fn product_deprecate(&self, ean: i64, deprecated: bool) -> zbus::Result<()>;
    async fn product_metadata_get(&self, ean: i64) -> zbus::Result<ProductMetadata>;
    async fn product_metadata_set(&self, ean: i64, metadata: ProductMetadata) -> zbus::Result<()>;
    async fn products_search(&self, search_query: &str) -> zbus::Result<Vec<Product>>;
    async fn get_restocks(&self, ean: i64, descending: bool) -> zbus::Result<Vec<RestockEntry>>;
    async fn get_last_restock(&self, ean: i64, min_price: u32) -> zbus::Result<RestockEntry>;
    async fn bestbeforelist(&self) -> zbus::Result<Vec<BestBeforeEntry>>;
    async fn get_supplier_list(&self) -> zbus::Result<Vec<Supplier>>;
    async fn get_supplier_product_list(&self, id: i32) -> zbus::Result<Vec<ProductInfo>>;
    async fn get_supplier_restock_dates(&self, id: i32) -> zbus::Result<Vec<i64>>;
    async fn add_supplier(&self, name: &str, postal_code: &str, city: &str, street: &str, phone: &str, website: &str) -> zbus::Result<()>;
    async fn get_supplier(&self, id: i32) -> zbus::Result<Supplier>;
    async fn ean_alias_list(&self) -> zbus::Result<Vec<EanAlias>>;
    async fn ean_alias_get(&self, ean: i64) -> zbus::Result<i64>;
    async fn ean_alias_add(&self, ean: i64, real_ean: i64) -> zbus::Result<()>;
    async fn new_product(&self, ean: i64, name: &str, category: i32, memberprice: i32, guestprice: i32) -> zbus::Result<()>;
    async fn check_user_password(&self, userid: i32, password: &str) -> zbus::Result<bool>;
    async fn set_user_password(&self, userid: i32, password: &str) -> zbus::Result<()>;
    async fn get_username(&self, userid: i32) -> zbus::Result<String>;
    async fn get_user_auth(&self, userid: i32) -> zbus::Result<UserAuth>;
    async fn set_user_auth(&self, auth: UserAuth) -> zbus::Result<()>;
    async fn get_member_ids(&self) -> zbus::Result<Vec<i32>>;
    async fn get_system_member_ids(&self) -> zbus::Result<Vec<i32>>;
    async fn get_user_info(&self, userid: i32) -> zbus::Result<UserInfo>;
    #[zbus(name="set_userTheme")]
    async fn set_user_theme(&self, userid: i32, theme: &str) -> zbus::Result<()>;
    async fn get_invoice(&self, userid: i32, from: i64, to: i64) -> zbus::Result<Vec<InvoiceEntry>>;
    async fn get_user_sale_stats(&self, user: i32, timecode: &str) -> zbus::Result<Vec<UserSaleStatsEntry>>;
	async fn get_first_purchase(&self, user: i32) -> zbus::Result<i64>;
	async fn get_last_purchase(&self, user: i32) -> zbus::Result<i64>;
    async fn cashbox_status(&self) -> zbus::Result<i32>;
    async fn cashbox_history(&self) -> zbus::Result<Vec<CashboxDiff>>;
    async fn cashbox_changes(&self, start: i64, stop: i64) -> zbus::Result<Vec<CashboxDiff>>;
    async fn cashbox_add(&self, user: i32, amount: i32, timestamp: i64) -> zbus::Result<()>;
    async fn get_category_list(&self) -> zbus::Result<Vec<ProductCategory>>;
    async fn user_exists(&self, user: i32) -> zbus::Result<bool>;
    async fn user_equals(&self, info: &UserInfo) -> zbus::Result<bool>;
    async fn user_disable(&self, user: i32, value: bool) -> zbus::Result<()>;
    async fn user_replace(&self, info: &UserInfo) -> zbus::Result<()>;
    async fn user_is_disabled(&self, user: i32) -> zbus::Result<bool>;
    async fn get_sales(&self, from: i64, to: i64) -> zbus::Result<Vec<SalesEntry>>;
}

#[proxy(
    interface = "io.mainframe.shopsystem.AudioPlayer",
    default_service = "io.mainframe.shopsystem.AudioPlayer",
    default_path = "/io/mainframe/shopsystem/audio"
)]
trait ShopAudio {
    async fn get_user_themes(&self) -> zbus::Result<Vec<String>>;
}

async fn get_user_themes() -> zbus::Result<Vec<String>> {
    let connection = Connection::system().await?;
    let proxy = ShopAudioProxy::new(&connection).await?;
    proxy.get_user_themes().await
}

#[proxy(
    interface = "io.mainframe.shopsystem.PGP",
    default_service = "io.mainframe.shopsystem.PGP",
    default_path = "/io/mainframe/shopsystem/pgp"
)]
trait ShopPGP {
	async fn import_archive(&self, data: Vec<u8>) -> zbus::Result<Vec<String>>;
	async fn list_keys(&self) -> zbus::Result<Vec<String>>;
	async fn get_key(&self, fingerprint: &str) -> zbus::Result<String>;
}

async fn import_archive(data: Vec<u8>) -> zbus::Result<Vec<String>> {
    let connection = Connection::system().await?;
    let proxy = ShopPGPProxy::new(&connection).await?;
    proxy.import_archive(data).await
}

async fn set_sessionid(uid: i32, sessionid: &str) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.set_sessionid(uid, sessionid).await
}

async fn get_user_by_sessionid(sessionid: &str) -> zbus::Result<i32> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_user_by_sessionid(sessionid).await
}

async fn get_username(uid: i32) -> zbus::Result<String> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_username(uid).await
}

async fn get_stock() -> zbus::Result<Vec<StockItem>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_stock().await
}

async fn get_productlist() -> zbus::Result<Vec<DetailedProductInfo>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_productlist().await
}

async fn get_prices(ean: i64) -> zbus::Result<Vec<PriceInfo>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_prices(ean).await
}

async fn get_product_aliases(ean: i64) -> zbus::Result<Vec<i64>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_product_aliases(ean).await
}

async fn ean_alias_get(ean: i64) -> zbus::Result<i64> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.ean_alias_get(ean).await
}

async fn ean_alias_add(ean: i64, real_ean: i64) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.ean_alias_add(ean, real_ean).await
}

async fn new_product(ean: i64, name: &str, category: i32, memberprice: i32, guestprice: i32) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.new_product(ean, name, category, memberprice, guestprice).await
}

async fn get_product_name(ean: i64) -> zbus::Result<String> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_product_name(ean).await
}

async fn get_product_amount(ean: i64) -> zbus::Result<i32> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_product_amount(ean).await
}

async fn get_product_amount_with_container_size(ean: i64) -> zbus::Result<(i32, u32)> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_product_amount_with_container_size(ean).await
}

async fn get_product_sales_info(ean: i64, since: i64) -> zbus::Result<u32> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_product_sales_info(ean, since).await
}

async fn get_product_category(ean: i64) -> zbus::Result<String> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_product_category(ean).await
}

async fn get_product_deprecated(ean: i64) -> zbus::Result<bool> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_product_deprecated(ean).await
}

async fn product_deprecate(ean: i64, deprecated: bool) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.product_deprecate(ean, deprecated).await
}

async fn product_metadata_get(ean: i64) -> zbus::Result<ProductMetadata> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.product_metadata_get(ean).await
}

async fn product_metadata_set(ean: i64, metadata: ProductMetadata) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.product_metadata_set(ean, metadata).await
}

async fn restock(user: i32, product: i64, amount: u32, price: u32, supplier: i32, best_before_date: i64) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.restock(user, product, amount, price, supplier, best_before_date).await
}

async fn buy(user: i32, product: i64) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.buy(user, product).await
}

async fn new_price(product: i64, timestamp: i64, memberprice: i32, guestprice: i32) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.new_price(product, timestamp, memberprice, guestprice).await
}

async fn products_search(search_query: &str) -> zbus::Result<Vec<Product>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.products_search(search_query).await
}

async fn get_restocks(ean: i64, descending: bool) -> zbus::Result<Vec<RestockEntry>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_restocks(ean, descending).await
}

async fn get_last_restock(ean: i64, min_price: u32) -> zbus::Result<RestockEntry> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_last_restock(ean, min_price).await
}

async fn get_bestbeforelist() -> zbus::Result<Vec<BestBeforeEntry>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.bestbeforelist().await
}

async fn cashbox_status() -> zbus::Result<i32> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.cashbox_status().await
}

async fn cashbox_history() -> zbus::Result<Vec<CashboxDiff>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.cashbox_history().await
}

async fn cashbox_history_named() -> zbus::Result<Vec<NamedCashboxDiff>> {
    let mut history = Vec::new();
    for change in cashbox_history().await? {
        history.push(NamedCashboxDiff {
            username: if change.user == -3 && change.amount < 0 { String::from("Loss") } else if change.user == -3 { String::from("Donation") } else { get_username(change.user).await.expect("failed to get username") },
            amount: change.amount,
            timestamp: change.timestamp,
        });
    }
    Ok(history)
}

async fn cashbox_changes(start: i64, stop: i64) -> zbus::Result<Vec<CashboxDiff>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.cashbox_changes(start, stop).await
}

async fn cashbox_add(user: i32, amount: i32, timestamp: i64) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.cashbox_add(user, amount, timestamp).await
}

async fn get_category_list() -> zbus::Result<Vec<ProductCategory>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_category_list().await
}

async fn user_exists(user: i32) -> zbus::Result<bool> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.user_exists(user).await
}

async fn user_is_disabled(user: i32) -> zbus::Result<bool> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.user_is_disabled(user).await
}

async fn user_disable(user: i32, value: bool) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.user_disable(user, value).await
}

async fn user_replace(info: &UserInfo) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.user_replace(info).await
}

async fn user_equals(info: &UserInfo) -> zbus::Result<bool> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.user_equals(info).await
}

async fn ean_alias_list() -> zbus::Result<Vec<EanAliasWithName>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;

    let list = proxy.ean_alias_list().await?;

    let mut result = Vec::new();

    for alias in list {
        result.push(EanAliasWithName {
            ean: alias.ean,
            real_ean: alias.real_ean,
            name: get_product_name(alias.real_ean).await?
        });
    }
    
    Ok(result)
}

async fn get_supplier(id: i32) -> zbus::Result<Supplier> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_supplier(id).await
}

async fn add_supplier(name: &str, postal_code: &str, city: &str, street: &str, phone: &str, website: &str) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.add_supplier(name, postal_code, city, street, phone, website).await
}

async fn get_supplier_list() -> zbus::Result<Vec<Supplier>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_supplier_list().await
}

async fn get_supplier_product_list(id: i32) -> zbus::Result<Vec<ProductInfo>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_supplier_product_list(id).await
}

async fn get_supplier_restock_dates(id: i32) -> zbus::Result<Vec<i64>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_supplier_restock_dates(id).await
}

async fn get_member_ids() -> zbus::Result<Vec<i32>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_member_ids().await
}

async fn get_user_list(system: bool) -> zbus::Result<Vec<(i32, String)>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;

    let list = if system { proxy.get_system_member_ids().await? } else {proxy.get_member_ids().await?};

    let mut result = Vec::new();

    for uid in list {
        result.push(
            (uid, proxy.get_username(uid).await?)
        );
    }
    
    Ok(result)
}

async fn get_user_auth(uid: i32) -> zbus::Result<UserAuth> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_user_auth(uid).await
}

async fn set_user_auth(authdata: UserAuth) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.set_user_auth(authdata).await
}

async fn set_user_password(userid: i32, theme: &str) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.set_user_password(userid, theme).await
}

async fn set_user_theme(userid: i32, theme: &str) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.set_user_theme(userid, theme).await
}

async fn get_user_info(uid: i32) -> zbus::Result<UserInfo> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_user_info(uid).await
}

async fn get_user_purchase_info(uid: i32) -> zbus::Result<(i64, i64)> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    let first = proxy.get_first_purchase(uid).await?;
    let last = proxy.get_last_purchase(uid).await?;
    Ok((first, last))
}

async fn get_invoice(uid: i32, start: i64, stop: i64) -> zbus::Result<Vec<InvoiceEntry>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_invoice(uid, start, stop).await
}

async fn get_user_sale_stats(uid: i32, timecode: &str) -> zbus::Result<Vec<UserSaleStatsEntry>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_user_sale_stats(uid, timecode).await
}

async fn check_user_password(userid: i32, password: &str) -> zbus::Result<bool> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.check_user_password(userid, password).await
}

async fn get_session_with_sessionid(sessionid: &str) -> zbus::Result<Session> {
    let uid = match get_user_by_sessionid(sessionid).await {
        Err(_) => { return Ok(Session {
            uid: 0,
            name: String::from("Guest"),
            superuser: false,
            auth_cashbox: false,
            auth_products: false,
            auth_users: false,
        })},
        Ok(uid) => uid,
    };

    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;

    let name = proxy.get_username(uid).await?;
    let auth = proxy.get_user_auth(uid).await?;

    Ok(Session {
        uid: auth.id,
        name: name,
        superuser: auth.superuser,
        auth_cashbox: auth.auth_cashbox || auth.superuser,
        auth_products: auth.auth_products || auth.superuser,
        auth_users: auth.auth_users || auth.superuser,
    })
}

async fn get_sales(start: i64, stop: i64) -> zbus::Result<Vec<SalesEntry>> {
    let connection = Connection::system().await?;
    let proxy = ShopDBProxy::new(&connection).await?;
    proxy.get_sales(start, stop).await
}

async fn get_session(cookies: &CookieJar<'_>) -> zbus::Result<Session> {
    get_session_with_sessionid(&cookies.get("sessionid").map(|c| c.value()).unwrap_or("invalid")).await
}

fn generate_session_id() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(19)
        .map(char::from)
        .collect()
}

#[post("/login", data = "<logindata>")]
async fn login(logindata: Form<UserLoginData<'_>>, cookies: &CookieJar<'_>) -> Result<Template, WebShopError> {
    let correctpw = check_user_password(logindata.userid, logindata.password).await?;
    match correctpw {
        false => Ok(Template::render("wrong-password", context! { page: "login", userid: logindata.userid })),
        true => {
            let sessionid = generate_session_id();
            match set_sessionid(logindata.userid, &sessionid).await {
                Err(error) => Ok(Template::render("error", context! { page: "error", errmsg: error.to_string() })),
                _ => {
                    cookies.add(Cookie::new("sessionid", format!("{}", sessionid)));
                    let session = get_session_with_sessionid(&sessionid).await?;
                    Ok(Template::render("login", context! { page: "login", session: session }))
                }
            }
        },
    }
}

#[get("/logout")]
async fn logout(cookies: &CookieJar<'_>) -> Template {
        cookies.remove(Cookie::from("sessionid"));
        Template::render("logout", context! { page: "logout"} )
}

#[get("/")]
async fn index(cookies: &CookieJar<'_>) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;
    Ok(Template::render("index", context! { page: "index", session: session }))
}

#[get("/products")]
async fn products(cookies: &CookieJar<'_>) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;

    let categories = get_category_list().await?;

    let stock = get_productlist().await?;
    Ok(Template::render("products/index", context! { page: "products/index", session: session, categories: categories, products: stock }))
}

fn check_valid_gtin(ean: i64, length: usize) -> bool {
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

fn parse_price(price: &str) -> Result<i32, ParseIntError> {
    let mut sum: i32 = 0;
    let mut split = price.split(['.', ',']);
    let part1 = split.nth(0);
    let part2 = split.nth(0);
    let part3 = split.nth(0);
    if part3.is_some() {
        /* generate ParseIntError */
        "invalid".parse::<i32>()?;
    } else if part2.is_some() {
        /* convert to cents */
        sum += part1.unwrap().parse::<i32>()? * 100;
        sum += part2.unwrap().parse::<i32>()?;
    } else {
        /* value in cents */
        sum += part1.unwrap().parse::<i32>()?;
    }
    Ok(sum)
}

#[post("/products/new", data = "<info>")]
async fn product_new(cookies: &CookieJar<'_>, info: Form<NewProduct>) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;

    if !session.superuser && !session.auth_products {
        return Err(WebShopError::PermissionDenied());
    }

    if !check_valid_gtin(info.id, 8) && !check_valid_gtin(info.id, 13) {
        return Ok(Template::render("error", context! { page: "error", session: session, errmsg: format!("Product ID '{}' is neither a valid EAN-8 nor EAN-13", info.id) }));
    }

    let name = get_product_name(info.id).await;
    if name.is_ok() {
        let name = name.unwrap();
        return Ok(Template::render("error", context! { page: "error", session: session, errmsg: format!("Product already exists: <a href=\"/products/{}\">{}</a>", info.id, name) }));
    }

    let alias = ean_alias_get(info.id).await?;

    if alias != info.id {
        return Ok(Template::render("error", context! { page: "error", session: session, errmsg: format!("This EAN is already handled as alias: {} ➔ <a href=\"/products/{}\">{}</a>", info.id, alias, alias) }));
    }

    let memberprice = parse_price(&info.memberprice);
    if memberprice.is_err() {
        return Ok(Template::render("error", context! { page: "error", session: session, errmsg: format!("memberprice ('{}') is not parsable", info.memberprice) }));
    }
    let memberprice = memberprice.unwrap();

    let guestprice = parse_price(&info.guestprice);
    if guestprice.is_err() {
        return Ok(Template::render("error", context! { page: "error", session: session, errmsg: format!("guestprice ('{}') is not parsable", info.guestprice) }));
    }
    let guestprice = guestprice.unwrap();

    if info.name.is_empty() {
        return Ok(Template::render("error", context! { page: "error", session: session, errmsg: format!("missing product name") }));
    }

    new_product(info.id, &info.name, info.category, memberprice, guestprice).await?;
    Ok(Template::render("products/new", context! { page: "products/new", session: session, ean: info.id, name: &info.name }))
}

#[get("/products/bestbefore")]
async fn product_bestbefore(cookies: &CookieJar<'_>) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;

    let list = get_bestbeforelist().await?;
    Ok(Template::render("products/bestbefore", context! { page: "products/bestbefore", session: session, list: list }))
}

#[get("/products/inventory")]
async fn product_inventory(cookies: &CookieJar<'_>) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;

    if !session.superuser && !session.auth_products {
        return Err(WebShopError::PermissionDenied());
    }

    let sysusers = get_user_list(true).await?;
    let suppliers = get_supplier_list().await?;
    let stock = get_stock().await?;

    Ok(Template::render("products/inventory", context! { page: "products/inventory", session: session, sysusers: sysusers, suppliers: suppliers, products: stock }))
}

async fn product_inventory_apply_helper(cookies: &CookieJar<'_>, data: Json<InventoryData>) -> zbus::Result<()> {
    let session = get_session(cookies).await?;

    if session.auth_products {
        for operation in &data.operations {
            if operation.diff > 0 {
                restock(session.uid, operation.ean, operation.diff as u32, 0, data.supplier, 0).await?;
            } else if operation.diff < 0 {
                let count = operation.diff.abs();
                for _ in 0..count {
                    buy(data.user, operation.ean).await?;
                }
            }
        }
    }

    Ok(())
}

#[post("/products/inventory/apply", format = "application/json", data = "<data>")]
async fn product_inventory_apply(cookies: &CookieJar<'_>, data: Json<InventoryData>) -> Result<Json<()>, Forbidden<String>> {
    match product_inventory_apply_helper(cookies, data).await {
        Err(error) => Err(Forbidden(error.to_string())),
        Ok(_) => Ok(Json(())),
    }
}

#[get("/products/<ean>/json", rank=1)]
async fn product_details_json(ean: i64) -> Result<Json<ProductDetails>, WebShopError> {
    let ean = ean_alias_get(ean).await?;

    Ok(Json(ProductDetails {
        ean: ean,
        name: get_product_name(ean).await?,
        category: get_product_category(ean).await?,
        amount: get_product_amount(ean).await?,
        deprecated: get_product_deprecated(ean).await?,
    }))
}

#[get("/products/<ean>/amount", rank=1)]
async fn product_amount_json(ean: i64) -> Result<Json<(i32, u32)>, WebShopError> {
    Ok(Json(get_product_amount_with_container_size(ean).await?))
}

#[get("/products/<ean>/sales-info?<timestamp>", rank=1)]
async fn product_sales_info_json(ean: i64, timestamp: i64) -> Result<Json<u32>, WebShopError> {
    Ok(Json(get_product_sales_info(ean, timestamp).await?))
}

#[get("/products/search/<search>", rank=2)]
async fn product_search_json(search: &str) -> Result<Json<Vec<Product>>, WebShopError> {
    Ok(Json(products_search(search).await?))
}

async fn product_missing(cookies: &CookieJar<'_>, ean: i64) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;
    let categories = get_category_list().await?;
    Ok(Template::render("products/missing", context! { page: "products/missing", session: session, ean: ean, categories: categories }))
}

#[get("/products/restock")]
async fn product_restock(cookies: &CookieJar<'_>) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;

    if !session.superuser && !session.auth_products {
        return Err(WebShopError::PermissionDenied());
    }

    let suppliers = get_supplier_list().await?;

    Ok(Template::render("products/restock", context! { page: "products/restock", session: session, suppliers: suppliers }))
}

#[get("/products/<ean>")]
async fn product_details(cookies: &CookieJar<'_>, ean: i64) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;
    let ean = ean_alias_get(ean).await?;
    let name;
    match get_product_name(ean).await {
        Ok(result) => name = result,
        Err(err) => {
            match err {
                zbus::Error::MethodError(ref errname, ref errmsg, _) => {
                    if errname.inner().as_str() == "io.mainframe.shopsystem.Database.SQL" && *errmsg == Some("Query returned no rows".to_string()) {
                        return product_missing(cookies, ean).await;
                    } else {
                        return Err(WebShopError::DBusError(err));
                    }
                },
                _ => {
                    return Err(WebShopError::DBusError(err));
                }
            }
        }
    }
    let aliases = get_product_aliases(ean).await?;
    let category = get_product_category(ean).await?;
    let amount = get_product_amount(ean).await?;
    let deprecated = get_product_deprecated(ean).await?;
    let prices = get_prices(ean).await?;
    let rawrestock = get_restocks(ean, false).await?;
    let metadata = product_metadata_get(ean).await.ok().unwrap_or_default();

    let mut restock = Vec::new();
    for entry in rawrestock {
        let supplierinfo = get_supplier(entry.supplier).await?;

        restock.push(RestockEntryNamedSupplier {
            timestamp: entry.timestamp,
            amount: entry.amount,
            price: entry.price,
            supplier: supplierinfo.name,
            best_before_date: entry.best_before_date,
        });
    }

    let suppliers = get_supplier_list().await?;

    Ok(Template::render("products/details", context! { page: "products/details", session: session, ean: ean, aliases: aliases, name: name, category: category, amount: amount, deprecated: deprecated, prices: prices, restock: restock, suppliers: suppliers, metadata: metadata }))
}

#[get("/products/<ean>/deprecate/<deprecated>")]
async fn web_product_deprecate(cookies: &CookieJar<'_>, ean: i64, deprecated: bool) -> Result<Json<bool>, Forbidden<String>> {
    let session = match get_session(cookies).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(session) => session,
    };

    if !session.superuser && !session.auth_products {
        return Err(Forbidden("Missing Permission".to_string()));
    }

    match product_deprecate(ean, deprecated).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(_) => {},
    };

    Ok(Json(deprecated))
}

#[post("/products/<ean>/add-prices", format = "application/json", data = "<priceinfo>")]
async fn web_product_add_prices(cookies: &CookieJar<'_>, ean: i64, priceinfo: Json<PriceInfo>) -> Result<Json<PriceInfo>, Forbidden<String>> {
    let session = match get_session(cookies).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(session) => session,
    };

    if !session.superuser && !session.auth_products {
        return Err(Forbidden("Missing Permission".to_string()));
    }

    let now = chrono::offset::Local::now().timestamp();

    match new_price(ean, now, priceinfo.memberprice, priceinfo.guestprice).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(_) => {},
    };

    Ok(Json(PriceInfo {
        timestamp: now,
        memberprice: priceinfo.memberprice,
        guestprice: priceinfo.guestprice,
    }))
}

#[post("/products/<ean>/restock", format = "application/json", data = "<data>")]
async fn web_product_restock(cookies: &CookieJar<'_>, ean: i64, data: Json<RestockEntry>) -> Result<Json<RestockEntryNamedSupplier>, Forbidden<String>> {
    let session = match get_session(cookies).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(session) => session,
    };

    if !session.superuser && !session.auth_products {
        return Err(Forbidden("Missing Permission".to_string()));
    }

    match restock(session.uid, ean, data.amount, data.price, data.supplier, data.best_before_date).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(_) => {},
    };

    let supplierinfo = match get_supplier(data.supplier).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(info) => info,
    };

    let now = chrono::offset::Local::now().timestamp();
    Ok(Json(RestockEntryNamedSupplier {
        timestamp: now,
        amount: data.amount,
        price: data.price,
        supplier: supplierinfo.name,
        best_before_date: data.best_before_date,
    }))
}

#[get("/products/<ean>/get-last-restock")]
async fn web_product_last_restock(cookies: &CookieJar<'_>, ean: i64) -> Result<Json<RestockEntry>, Forbidden<String>> {
    let session = match get_session(cookies).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(session) => session,
    };

    if !session.superuser && !session.auth_products {
        return Err(Forbidden("Missing Permission".to_string()));
    }

    let data = match get_last_restock(ean, 1).await {
        Ok(data) => Ok(data),
        Err(err) => Err(Forbidden(err.to_string())),
    }?;

    Ok(Json(data))
}

#[get("/products/<ean>/add-alias/<alias>", format = "application/json")]
async fn web_product_alias_add(cookies: &CookieJar<'_>, ean: i64, alias: i64) -> Result<Json<i64>, Forbidden<String>> {
    let session = match get_session(cookies).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(session) => session,
    };

    if !session.superuser && !session.auth_products {
        return Err(Forbidden("Missing Permission".to_string()));
    }

    /* verify the product exists */
    match get_product_name(ean).await {
        Err(_) => { return Err(Forbidden(String::from("product EAN does not exist"))); },
        Ok(_) => {},
    };

    if !check_valid_gtin(alias, 8) && !check_valid_gtin(alias, 13) {
        return Err(Forbidden(String::from("The supplied alias is neither a valid EAN-8 nor EAN-13.")));
    }

    /* verify the alias EAN does not yet exists */
    match ean_alias_get(alias).await {
        Err(e) => { return Err(Forbidden(String::from(e.to_string()))); },
        Ok(val) => {
            if val != alias {
                return Err(Forbidden(String::from("The new EAN already exists as alias")));
            }
        },
    };

    match get_product_name(alias).await {
        Err(_) => { },
        Ok(_) => { return Err(Forbidden(String::from("The new EAN already exists as product"))); },
    };

    match ean_alias_add(alias, ean).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(_) => {},
    };

    Ok(Json(alias))
}

#[get("/products/<ean>/metadata-get")]
async fn web_product_metadata_get(_cookies: &CookieJar<'_>, ean: i64) -> Result<Json<ProductMetadata>, Forbidden<String>> {
    match product_metadata_get(ean).await {
        Ok(metadata) => Ok(Json(metadata)),
        Err(err) => Err(Forbidden(err.to_string())),
    }
}

#[post("/products/<ean>/metadata-set", format = "application/json", data = "<metadata>")]
async fn web_product_metadata_set(cookies: &CookieJar<'_>, ean: i64, metadata: Json<ProductMetadata>) -> Result<Json<()>, Forbidden<String>> {
    let session = match get_session(cookies).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(session) => session,
    };

    if !session.superuser && !session.auth_products {
        return Err(Forbidden("Missing Permission".to_string()));
    }

    let metadata = metadata.into_inner();

    match product_metadata_set(ean, metadata).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(_) => {},
    };

    Ok(Json(()))
}

#[get("/suppliers/order-suggestion")]
async fn web_product_order_suggestion_step1(cookies: &CookieJar<'_>) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;
    let suppliers = get_supplier_list().await?;

    Ok(Template::render("suppliers/order-suggestion-selection", context! { page: "suppliers/order-suggestion", session: session, suppliers: suppliers }))
}

#[get("/suppliers/<id>/order-suggestion")]
async fn web_product_order_suggestion_step2(cookies: &CookieJar<'_>, id: i32) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;
    let supplier_name = get_supplier(id).await?.name;

    Ok(Template::render("suppliers/order-suggestion", context! { page: "suppliers/order-suggestion", session: session, supplier: id, supplier_name: supplier_name }))
}

#[get("/aliases")]
async fn aliases(cookies: &CookieJar<'_>) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;
    let list = ean_alias_list().await?;

    Ok(Template::render("aliases/index", context! { page: "aliases/index", session: session, list: list }))
}

#[get("/suppliers")]
async fn suppliers(cookies: &CookieJar<'_>) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;
    let list = get_supplier_list().await?;

    Ok(Template::render("suppliers/index", context! { page: "suppliers/index", session: session, list: list }))
}

#[get("/suppliers/list", format = "application/json")]
async fn supplier_json_list(_cookies: &CookieJar<'_>) -> Result<Json<Vec<Supplier>>, Forbidden<String>> {
    let list = match get_supplier_list().await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(list) => list,
    };

    Ok(Json(list))
}

#[get("/suppliers/<id>/product-list", format = "application/json")]
async fn supplier_json_product_list(_cookies: &CookieJar<'_>, id: i32) -> Result<Json<Vec<ProductInfo>>, Forbidden<String>> {
    let list = match get_supplier_product_list(id).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(list) => list,
    };

    Ok(Json(list))
}

#[get("/suppliers/<id>/restock-dates", format = "application/json")]
async fn supplier_json_restock_dates(_cookies: &CookieJar<'_>, id: i32) -> Result<Json<Vec<i64>>, Forbidden<String>> {
    let list = match get_supplier_restock_dates(id).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(list) => list,
    };

    Ok(Json(list))
}

#[post("/suppliers/new", data = "<info>")]
async fn web_suppliers_new(cookies: &CookieJar<'_>, info: Form<NewSupplier>) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;

    if !session.superuser && !session.auth_products {
        return Err(WebShopError::PermissionDenied());
    }

    add_supplier(&info.name.clone(), &info.postal_code, &info.city, &info.street, &info.phone, &info.website).await?;
    Ok(Template::render("suppliers/new", context! { page: "suppliers/new", session: session, name: &info.name }))
}

#[get("/cashbox/status")]
async fn cashbox_state(cookies: &CookieJar<'_>) -> Result<Json<i32>, Forbidden<String>> {
    let session = match get_session(cookies).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(session) => session,
    };

    if !session.superuser && !session.auth_cashbox {
        return Err(Forbidden("Missing Permission".to_string()));
    }

    let cashbox_status = match cashbox_status().await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(status) => status,
    };

    Ok(Json(cashbox_status))
}

#[get("/cashbox/history")]
async fn cashbox_history_json(cookies: &CookieJar<'_>) -> Result<Json<Vec<NamedCashboxDiff>>, Forbidden<String>> {
    let session = match get_session(cookies).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(session) => session,
    };

    if !session.superuser && !session.auth_cashbox {
        return Err(Forbidden("Missing Permission".to_string()));
    }

    let cashbox_history = match cashbox_history_named().await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(history) => history,
    };

    Ok(Json(cashbox_history))
}

#[get("/cashbox")]
async fn cashbox(cookies: &CookieJar<'_>) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;

    if !session.superuser && !session.auth_cashbox {
        return Err(WebShopError::PermissionDenied());
    }

    let cashbox_history = cashbox_history_named().await?;

    Ok(Template::render("cashbox/index", context! { page: "cashbox/index", session: session, cashbox_history: cashbox_history }))
}

#[post("/cashbox/update", format = "application/json", data = "<data>")]
async fn cashbox_update(cookies: &CookieJar<'_>, data: Json<CashboxUpdate>) -> Result<Json<()>, Forbidden<String>> {
    let session = match get_session(cookies).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(session) => session,
    };

    if !session.superuser && !session.auth_cashbox {
        return Err(Forbidden("Missing Permission".to_string()));
    }

    let now = chrono::offset::Local::now().timestamp();
    let amount = if data.amount >= 0 { data.amount } else { -data.amount };

    let result = match data.update_type {
        CashboxUpdateType::Withdrawal => cashbox_add(session.uid, -amount, now),
        CashboxUpdateType::Deposit => cashbox_add(session.uid, amount, now),
        CashboxUpdateType::Loss => cashbox_add(-3, -amount, now),
        CashboxUpdateType::Donation => cashbox_add(-3, amount, now),
    }.await;

    match result {
        Err(error) => Err(Forbidden(error.to_string())),
        Ok(_) => Ok(Json(())),
    }
}

#[get("/cashbox/details/<year>/<month>")]
async fn cashbox_details(cookies: &CookieJar<'_>, year: i32, month: u32) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;

    if !session.superuser && !session.auth_cashbox {
        return Err(WebShopError::PermissionDenied());
    }

    let now = chrono::offset::Local::now();
    let year = if year <= 0 || year > 10000 { now.year() } else { year };
    let month = if month == 0 || month > 12 { now.month() } else { month };

    let monthname = [ "All Months", "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December" ][month as usize];

    let start = chrono::Local.with_ymd_and_hms(year, month, 1, 0, 0, 0).unwrap();
    let stop = start.checked_add_months(chrono::Months::new(1)).unwrap();
    let start = start.timestamp();
    let stop = stop.timestamp();

    let changes = cashbox_changes(start, stop).await?;
    let invoice = get_invoice(0, start, stop).await?;

    let mut debit = 0;
    for e in invoice {
        debit += e.price;
    }

    let mut withdrawal_list = Vec::new();
    let mut donation_list = Vec::new();
    let mut loss_list = Vec::new();
    let mut loss = 0;
    let mut donation = 0;
    let mut withdrawal = 0;
    for change in changes {
        if change.user == -3 && change.amount < 0 {
            loss += change.amount;
            loss_list.push(change);
        } else if change.user == -3 {
            donation += change.amount;
            donation_list.push(change);
        } else {
            withdrawal += change.amount;
            withdrawal_list.push(NamedCashboxDiff {
                username: get_username(change.user).await.expect("failed to get username"),
                amount: change.amount,
                timestamp: change.timestamp,
            });
        }
    }

    Ok(Template::render("cashbox/details", context! { page: "cashbox/details", session: session, debit: debit, loss: loss, donation: donation, withdrawal: withdrawal, month: monthname, year: year, withdrawal_list: withdrawal_list, donation_list: donation_list, loss_list: loss_list }))
}

#[get("/users")]
async fn users(cookies: &CookieJar<'_>) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;

    if !session.superuser && !session.auth_users {
        return Err(WebShopError::PermissionDenied());
    }

    let userlist = get_user_list(false).await?;
    Ok(Template::render("users/index", context! { page: "users/index", session: session, list: userlist }))
}

#[get("/users/<id>")]
async fn user_info(cookies: &CookieJar<'_>, id: i32) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;

    if !session.superuser && !session.auth_users && id != session.uid {
        return Err(WebShopError::PermissionDenied());
    }

    let userinfo = get_user_info(id).await?;
    let userauth = get_user_auth(id).await?;
    let sound_themes = get_user_themes().await?;

    Ok(Template::render("users/info", context! { page: "users/info", userinfo: userinfo, userauth: userauth, sound_themes: sound_themes, session: session }))
}

fn render_centered_text(ctx: &cairo::Context, x: f64, y: f64, w: i32, msg: &str) -> Result<(), cairo::Error> {
    ctx.save()?;
    ctx.move_to(x, y);
    ctx.set_source_rgb(0.0, 0.0, 0.0);

    /* get pango layout */
    let layout = pangocairo::functions::create_layout(&ctx);

    /* setup font */
    let mut font = pango::FontDescription::new();
    font.set_family("LMRoman12");
    font.set_size(18 * pango::SCALE);
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

#[get("/users/<id>/barcode.svg")]
async fn user_barcode(_cookies: &CookieJar<'_>, id: i32) -> Result<(ContentType, String), WebShopError> {
    let barcodesvg = SVG::new(100)
        .xdim(2)
        .xmlns("http://www.w3.org/2000/svg".to_string())
        .foreground(Color::black())
        .background(Color::white());
    let barcodedata = Code39::with_checksum(format!("USER {}", id))?.encode();
    let barcode = barcodesvg.generate(&barcodedata)?;

    let buffer: std::io::Cursor<Vec<u8>> = Default::default();
    let document = cairo::SvgSurface::for_stream(500.0, 200.0, buffer)?;
    let rect = cairo::Rectangle::new(50.0, 0.0, 400.0, 150.0);
    let ctx = cairo::Context::new(&document)?;

    ctx.set_source_rgb(1.0, 1.0, 1.0);
    ctx.rectangle(0.0, 0.0, 500.0, 200.0);
    ctx.fill()?;

    let bytes = Bytes::from(barcode.as_bytes());
    let stream = gio::MemoryInputStream::from_bytes(&bytes);
    let handle = rsvg::Loader::new().read_stream(&stream, None::<&gio::File>, None::<&gio::Cancellable>)?;
    let renderer = rsvg::CairoRenderer::new(&handle);
    renderer.render_document(&ctx, &rect)?;

    let text = format!("User {}", id);
    render_centered_text(&ctx, 50.0, 150.0, 400, &text)?;

    document.flush();
    let result = document.finish_output_stream();

    match result {
        Ok(boxedstream) => {
            match boxedstream.downcast::<std::io::Cursor<Vec<u8>>>() {
                Ok(buffer) => Ok((ContentType::SVG, String::from_utf8(buffer.into_inner())?)),
                Err(_err) => Err(WebShopError::UnboxError("Failed to unbox".to_string())),
            }
        },
        Err(e) => {
            Err(e.error.into())
        },
    }
}

#[post("/users/set-sound-theme/<userid>", format = "application/json", data = "<theme>")]
async fn user_sound_theme_set(cookies: &CookieJar<'_>, userid: i32, theme: Json<String>) -> Result<Json<bool>, Forbidden<String>> {
    let session = match get_session(cookies).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(session) => session,
    };

    if !session.superuser && !session.auth_users && userid != !session.uid {
        return Err(Forbidden("Missing Permission".to_string()));
    }

    let theme = theme.into_inner();

    match set_user_theme(userid, &theme).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(_) => {},
    };

    Ok(Json(true))
}

#[post("/users/set-password/<userid>", format = "application/json", data = "<password>")]
async fn user_password_set(cookies: &CookieJar<'_>, userid: i32, password: Json<String>) -> Result<Json<bool>, Forbidden<String>> {
    let session = match get_session(cookies).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(session) => session,
    };

    if !session.superuser && !session.auth_users && userid != session.uid {
        return Err(Forbidden("Missing Permission".to_string()));
    }

    let password = password.into_inner();

    if password.is_empty() {
        return Ok(Json(false));
    }

    match set_user_password(userid, &password).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(_) => {},
    };

    Ok(Json(true))
}

#[get("/users/toggle-auth/<userid>/<permission>")]
async fn user_toggle_auth(cookies: &CookieJar<'_>, userid: i32, permission: String) -> Result<Json<UserAuth>, Forbidden<String>> {
    let session = match get_session(cookies).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(session) => session,
    };

    if !session.superuser && !session.auth_users {
        return Err(Forbidden("Missing Permission".to_string()));
    }

    let mut userauth = match get_user_auth(userid).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(userauth) => userauth,
    };

    match &permission[..] {
        "products" => { userauth.auth_products = !userauth.auth_products; },
        "cashbox" => { userauth.auth_cashbox = !userauth.auth_cashbox; },
        "users" => { userauth.auth_users = !userauth.auth_users; },
        _ => { return Err(Forbidden("Invalid Parameter".to_string())); },
    };

    match set_user_auth(userauth).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(_) => {},
    };

    Ok(Json(userauth))
}

pub fn get_days_from_month(year: i32, month: u32) -> u32 {
    NaiveDate::from_ymd_opt(
        match month {
            12 => year + 1,
            _ => year,
        },
        match month {
            12 => 1,
            _ => month + 1,
        },
        1,
    ).unwrap()
    .signed_duration_since(NaiveDate::from_ymd_opt(year, month, 1).unwrap())
    .num_days() as u32
}

#[get("/users/<user_id>/invoice/<year>/<month>/<day>")]
async fn user_invoice_full(cookies: &CookieJar<'_>, user_id: i32, year: i32, month: u32, day: u32) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;

    if !session.superuser && !session.auth_users && user_id != session.uid {
        return Err(WebShopError::PermissionDenied());
    }

    let (first, last) = get_user_purchase_info(user_id).await?;

    let first: chrono::DateTime<Utc> = chrono::DateTime::<Utc>::from_timestamp(first, 0).expect("invalid timestamp");
    let last: chrono::DateTime<Utc> = chrono::DateTime::<Utc>::from_timestamp(last, 0).expect("invalid timestamp");

    /* make sure parameters are sensible */
    let year = if year < first.year() { first.year() } else { year };
    let year = if year > last.year() { last.year() } else { year };
    let month = if month > 12 { 0 } else { month };
    let day = if day > 31 { 0 } else { day };

    let monthnames = [ "All Months", "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December" ];

    let start;
    let stop;
    if day != 0 {
        start = chrono::Local.with_ymd_and_hms(year, month, day, 8, 0, 0).unwrap();
        stop = start.checked_add_days(chrono::Days::new(1)).unwrap();
    } else if month != 0 {
        start = chrono::Local.with_ymd_and_hms(year, month, 1, 0, 0, 0).unwrap();
        stop = start.checked_add_months(chrono::Months::new(1)).unwrap();
    } else {
        start = chrono::Local.with_ymd_and_hms(year, 1, 1, 0, 0, 0).unwrap();
        stop = chrono::Local.with_ymd_and_hms(year+1, 1, 1, 0, 0, 0).unwrap();
    }

    /* generate month dropdown list */
    let mut monthlist = Vec::new();
    monthlist.push(DropdownEntry { id: 0, name: monthnames[0].to_string(), disabled: false, });
    if start.year() < first.year() || start.year() > last.year() {
        for i in 1..monthnames.len() {
            monthlist.push(DropdownEntry { id: i, name: monthnames[i].to_string(), disabled: true, });
        }
    } else if start.year() == first.year() {
        for i in 1..((first.month()) as usize) {
            monthlist.push(DropdownEntry { id: i, name: monthnames[i].to_string(), disabled: true, });
        }
        for i in ((first.month()) as usize)..monthnames.len() {
            monthlist.push(DropdownEntry { id: i, name: monthnames[i].to_string(), disabled: false, });
        }
    } else if start.year() == last.year() {
        for i in 1..((last.month()+1) as usize) {
            monthlist.push(DropdownEntry { id: i, name: monthnames[i].to_string(), disabled: false, });
        }
        for i in ((last.month()+1) as usize)..monthnames.len() {
            monthlist.push(DropdownEntry { id: i, name: monthnames[i].to_string(), disabled: true, });
        }
    } else {
        for i in 1..monthnames.len() {
            monthlist.push(DropdownEntry { id: i, name: monthnames[i].to_string(), disabled: false, });
        }
    }

    /* generate days dropdown list */
    let mut daylist = Vec::new();
    daylist.push(DropdownEntry { id: 0, name: "All days".to_string(), disabled: false, });
    if month == 0 {
        /* this case only supports 'All days' */
    } else if start.year() < first.year() || (start.year() == first.year() && start.month() < first.month()) || start.year() > last.year() || (start.year() == last.year() && start.month() > last.month()) {
        for i in 1..get_days_from_month(year, month)+1 {
            daylist.push(DropdownEntry { id: i as usize, name: format!("{}", i), disabled: true });
        }
    } else if start.year() == first.year() && start.month() == first.month() {
        for i in 1..((first.day()) as usize) {
            daylist.push(DropdownEntry { id: i, name: format!("{}", i), disabled: true, });
        }
        for i in ((first.day()) as usize)..((get_days_from_month(year, month)+1) as usize) {
            daylist.push(DropdownEntry { id: i, name: format!("{}", i), disabled: false, });
        }

    } else if start.year() == last.year() && start.month() == last.month() {
        for i in 1..((last.day()+1) as usize) {
            daylist.push(DropdownEntry { id: i, name: format!("{}", i), disabled: false, });
        }
        for i in ((last.day()+1) as usize)..((get_days_from_month(year, month)+1) as usize) {
            daylist.push(DropdownEntry { id: i as usize, name: format!("{}", i), disabled: true, });
        }
    } else {
        for i in 1..get_days_from_month(year, month)+1 {
            daylist.push(DropdownEntry { id: i as usize, name: format!("{}", i), disabled: false });
        }
    }

    let start = start.timestamp();
    let stop = stop.timestamp();

    let invoicedata = get_invoice(user_id, start, stop).await?;

    let mut sum = 0;
    for entry in &invoicedata {
        sum += entry.price;
    }

    let monthname = monthnames[month as usize];
    Ok(Template::render("users/invoice", context! { page: "users/invoice", user_id: user_id, firstyear: first.year(), lastyear: last.year(), year: year, month: month, day: day, monthname: monthname, monthlist: monthlist, daylist: daylist, invoicedata: invoicedata, sum: sum, session: session }))
}

#[get("/users/<user_id>/invoice")]
async fn user_invoice(cookies: &CookieJar<'_>, user_id: i32) -> Result<Template, WebShopError> {
    let now = chrono::offset::Local::now();
    user_invoice_full(cookies, user_id, now.year(), now.month(), now.day()).await
}

#[get("/users/<id>/stats")]
async fn user_stats(cookies: &CookieJar<'_>, id: i32) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;

    if !session.superuser && !session.auth_users && id != session.uid {
        return Err(WebShopError::PermissionDenied());
    }

    let sales_per_year = get_user_sale_stats(id, "%Y").await?;
    let sales_per_week = get_user_sale_stats(id, "%W").await?;
    let sales_per_weekday = get_user_sale_stats(id, "%w").await?;
    let sales_per_hour = get_user_sale_stats(id, "%H").await?;

    Ok(Template::render("users/stats", context! { page: "users/stats", session: session, sales_per_year: sales_per_year, sales_per_week: sales_per_week, sales_per_weekday: sales_per_weekday, sales_per_hour: sales_per_hour }))
}

#[get("/users/import")]
async fn user_import(cookies: &CookieJar<'_>) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;

    Ok(Template::render("users/import", context! { page: "users/import", session: session }))
}

async fn csvlist2changes(csvlist: &Vec<UserInfo>) -> Result<Vec<UserChange>, zbus::Error> {
    let mut changes = Vec::new();
    let mut csvmemberids = Vec::new();

    for csvmember in csvlist {
        let mut change = UserChange { old: None, new: None };
        if user_exists(csvmember.id).await.unwrap() && !user_equals(csvmember).await? {
            change.old = Some(get_user_info(csvmember.id).await?);
        }
        if !user_exists(csvmember.id).await.unwrap() || !user_equals(csvmember).await? {
            change.new = Some(csvmember.clone());
        }
        if change.new.is_some() || change.old.is_some() {
            changes.push(change);
        }
        csvmemberids.push(csvmember.id);
    }

    for memberid in get_member_ids().await? {
        if user_is_disabled(memberid).await? {
            continue;
        }

        if !csvmemberids.contains(&memberid) {
            changes.push(UserChange {
                old: Some(get_user_info(memberid).await?),
                new: None,
            });
        }
    }

    Ok(changes)
}

#[post("/users/import", data = "<data>")]
async fn user_import_upload(cookies: &CookieJar<'_>, data: Data<'_>) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;

    if !session.superuser && !session.auth_users {
        return Err(WebShopError::PermissionDenied());
    }

    /* Hack to get uploaded CSV file in memory */
    let stream = data.open(1.mebibytes());
    let data = stream.into_string().await?;

    if !data.is_complete() {
        return Ok(Template::render("error", context! { page: "error", session: session, errmsg: "incomplete upload".to_string() }));
    }

    let csvdata = data.value.split("\r\n\r\n").nth(1);
    let csvdata = match csvdata {
        None => { return Ok(Template::render("error", context! { page: "error", session: session, errmsg: "invalid upload".to_string() })); },
        Some(x) => x,
    };

    let csvdata = csvdata.split("\n\r\n").nth(0);
    let csvdata = match csvdata {
        None => { return Ok(Template::render("error", context! { page: "error", session: session, errmsg: "invalid upload".to_string() })); },
        Some(x) => x,
    };

    let csvlist = UserInfoList::from_csv(csvdata)?.data;
    let changes = csvlist2changes(&csvlist).await?;
    Ok(Template::render("users/import2", context! { page: "error", session: session, changes: changes }))
}

#[post("/users/import/apply", format = "application/json", data = "<change>")]
async fn user_import_apply(cookies: &CookieJar<'_>, change: Json<UserChange>) -> Result<Json<bool>, Forbidden<String>> {
    let session = match get_session(cookies).await {
        Err(error) => { return Err(Forbidden(error.to_string())); },
        Ok(session) => session,
    };

    if !session.superuser && !session.auth_users {
        return Err(Forbidden("Missing Permission".to_string()));
    }

    let result = if change.new.is_some() {
        user_replace(&change.new.as_ref().unwrap()).await
    } else if change.old.is_some() {
        user_disable(change.old.as_ref().unwrap().id, true).await
    } else {
        return Err(Forbidden("Invalid Change".to_string()));
    };

    match result {
        Err(error) => Err(Forbidden(error.to_string())),
        Ok(_) => Ok(Json(true)),
    }
}

#[get("/users/import-pgp")]
async fn user_import_pgp(cookies: &CookieJar<'_>) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;
    Ok(Template::render("users/import-pgp", context! { page: "users/import-pgp", session: session }))
}

#[post("/users/import-pgp", data = "<form>")]
async fn user_import_pgp_upload(cookies: &CookieJar<'_>, mut form: Form<FileUpload<'_>>) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;

    if !session.superuser && !session.auth_users {
        return Err(WebShopError::PermissionDenied());
    }

    if !form.file.is_complete() {
        return Ok(Template::render("error", context! { page: "error", session: session, errmsg: "Incomplete file upload!" }));
    }

    match form.file.persist_to("/tmp/shopsystem-pgp-keys.archive").await {
        Err(error) => { return Ok(Template::render("error", context! { page: "error", errmsg: error.to_string(), session: session })) },
        Ok(_) => {},
    };

    let archivedata = match std::fs::read("/tmp/shopsystem-pgp-keys.archive") {
        Err(error) => { return Ok(Template::render("error", context! { page: "error", errmsg: error.to_string(), session: session })) },
        Ok(data) => data,
    };

    let keys = import_archive(archivedata).await?;
    Ok(Template::render("users/import-pgp-upload", context! { page: "users/import-pgp-upload", session: session, keys: keys }))
}

fn get_unix_time() -> i64 {
    match std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH) {
        Ok(n) => n.as_secs().try_into().expect("Cannot convert timestamp from u64 to i64"),
        Err(_) => panic!("SystemTime before UNIX EPOCH!"),
    }
}


#[get("/sales")]
async fn sales(cookies: &CookieJar<'_>) -> Result<Template, WebShopError> {
    let session = get_session(cookies).await?;
    let sales = get_sales(get_unix_time() - 86400, get_unix_time()).await?;

    Ok(Template::render("sales/index", context! { page: "sales/index", session: session, sales: sales }))
}

#[catch(404)]
fn not_found() -> &'static str {
    "could not find the page (404)"
}

fn gendericon<S: BuildHasher>(value: &rocket_dyn_templates::tera::Value, _: &HashMap<String, rocket_dyn_templates::tera::Value, S>) -> rocket_dyn_templates::tera::Result<rocket_dyn_templates::tera::Value> {
    let val = rocket_dyn_templates::tera::try_get_value!("gendericon", "value", String, value);
    let result = match val.as_str() {
        "masculinum" => "<span class=\"bi-gender-male\"></span>",
        "femininum" => "<span class=\"bi-gender-female\"></span>",
        _ => "<span class=\"bi-question\"></span>",
    };
    Ok(rocket_dyn_templates::tera::to_value(result).unwrap())
}

fn cent2euro<S: BuildHasher>(value: &rocket_dyn_templates::tera::Value, _: &HashMap<String, rocket_dyn_templates::tera::Value, S>) -> rocket_dyn_templates::tera::Result<rocket_dyn_templates::tera::Value> {
    let cent = rocket_dyn_templates::tera::try_get_value!("cent2euro", "value", i32, value);
    let euro = cent / 100;
    let cent = (cent % 100).abs();
    let result = format!("{}.{:02}", euro, cent);
    Ok(rocket_dyn_templates::tera::to_value(result).unwrap())
}

fn togglebutton<S: BuildHasher>(args: &HashMap<String, rocket_dyn_templates::tera::Value, S>) -> rocket_dyn_templates::tera::Result<rocket_dyn_templates::tera::Value> {
    /* expects the following arguments: clickable, enabled, buttonid, enabledStr (default="Yes"), disabledStr (default = "No") */
    let clickable = match args.get("clickable") {
        Some(val) => match rocket_dyn_templates::serde::json::from_value::<bool>(val.clone()) {
            Ok(v) =>  v,
            Err(_) => { return Err("oops".into()) },
        },
        None => { return Err("oops".into()) },
    };
    let enabled = match args.get("enabled") {
        Some(val) => match rocket_dyn_templates::serde::json::from_value::<bool>(val.clone()) {
            Ok(v) =>  v,
            Err(_) => { return Err("oops".into()) },
        },
        None => { return Err("oops".into()) },
    };
    let buttonid = match args.get("buttonid") {
        Some(val) => match rocket_dyn_templates::serde::json::from_value::<String>(val.clone()) {
            Ok(v) =>  v,
            Err(_) => { return Err("oops".into()) },
        },
        None => { return Err("oops".into()) },
    };
    let enabled_str = match args.get("enabledStr") {
        Some(val) => match rocket_dyn_templates::serde::json::from_value::<String>(val.clone()) {
            Ok(v) =>  v,
            Err(_) => { return Err("oops".into()) },
        },
        None => { String::from("Yes") },
    };
    let disabled_str = match args.get("disabledStr") {
        Some(val) => match rocket_dyn_templates::serde::json::from_value::<String>(val.clone()) {
            Ok(v) =>  v,
            Err(_) => { return Err("oops".into()) },
        },
        None => { String::from("No") },
    };

    let clickable = if clickable { "" } else { "disabled" };
    let color = if enabled { "btn-success" } else { "btn-danger" };
    let buttonval = if enabled { enabled_str } else { disabled_str };

    let result = format!("<button id=\"{buttonid}\" class=\"btn {color} {clickable}\" type=\"button\">{buttonval}</button>");
    Ok(rocket_dyn_templates::tera::to_value(result).unwrap())
}

#[launch]
fn rocket() -> _ {
    let mut cfg = Ini::new();
    cfg.load("/etc/shopsystem/config.ini").expect("failed to load config");
    let path = cfg.get("GENERAL", "datapath").unwrap_or("/usr/share/shopsystem/".to_string());
    let templatepath = format!("{}/{}", path, "templates/");
    let staticpath = format!("{}/{}", path, "templates/static/");

    let figment = rocket::Config::figment()
        .merge(("template_dir", templatepath));

    rocket::custom(figment)
        .register("/", catchers![not_found])
        .mount("/static", rocket::fs::FileServer::from(staticpath))
        .mount("/", routes![login, logout, index, products, product_new, product_details,
            product_restock, product_search_json, product_details_json, product_amount_json,
            product_sales_info_json, web_product_deprecate, web_product_add_prices,
            web_product_restock, web_product_last_restock, web_product_alias_add,
            web_product_metadata_get, web_product_metadata_set,
            web_product_order_suggestion_step1, web_product_order_suggestion_step2,
            product_bestbefore, product_inventory, product_inventory_apply, aliases,
            suppliers, web_suppliers_new, supplier_json_list, supplier_json_product_list,
            supplier_json_restock_dates, cashbox, cashbox_state, cashbox_history_json,
            cashbox_update, cashbox_details, users, user_info, user_barcode,
            user_sound_theme_set, user_password_set, user_toggle_auth, user_invoice,
            user_invoice_full, user_stats, user_import, user_import_upload,
            user_import_apply, user_import_pgp, user_import_pgp_upload, sales])
        .attach(Template::custom(|engines| {
            engines.tera.register_filter("cent2euro", cent2euro);
            engines.tera.register_filter("gendericon", gendericon);
            engines.tera.register_function("togglebutton", togglebutton);
        }))
}
