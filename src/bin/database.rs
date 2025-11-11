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
use zbus::{connection, DBusError, interface};
use std::collections::HashMap;
use r2d2_sqlite::SqliteConnectionManager;
use r2d2_sqlite::rusqlite::OptionalExtension;
use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use hex::ToHex;
use configparser::ini::Ini;

struct Database {
    pool: r2d2::Pool<SqliteConnectionManager>,
}

#[derive(DBusError, Debug)]
#[zbus(prefix = "io.mainframe.shopsystem.Database")]
enum DatabaseError {
    #[zbus(error)]
    ZBus(zbus::Error),
    R2D2(String),
    SQL(String),
}

impl From<r2d2::Error> for DatabaseError {
    fn from(err: r2d2::Error) -> DatabaseError {
            DatabaseError::R2D2(err.to_string())
    }
}

impl From<r2d2_sqlite::rusqlite::Error> for DatabaseError {
    fn from(err: r2d2_sqlite::rusqlite::Error) -> DatabaseError {
            DatabaseError::SQL(err.to_string())
    }
}

#[derive(Deserialize,Serialize, zbus::zvariant::Type)]
struct DetailedProduct {
	ean: u64,
	name: String,
	category: String,
	amount: i32,
	memberprice: i32,
	guestprice: i32,
}

#[derive(Deserialize,Serialize, zbus::zvariant::Type)]
struct ProductInfo {
	ean: u64,
	name: String,
}

#[derive(Deserialize,Serialize, zbus::zvariant::Type)]
struct DetailedProductInfo {
	ean: u64,
    aliases: Vec<u64>,
	name: String,
	category: String,
	amount: i32,
	memberprice: i32,
	guestprice: i32,
    deprecated: bool,
}

#[derive(Deserialize,Serialize, zbus::zvariant::Type)]
struct PriceEntry {
	valid_from: i64,
	memberprice: i32,
	guestprice: i32,
}

#[derive(Deserialize,Serialize, zbus::zvariant::Type)]
struct RestockEntry {
	timestamp: i64,
	amount: u32,
	price: u32,
	supplier: i32,
	best_before_date: i64,
}

#[derive(Deserialize,Serialize, zbus::zvariant::Type)]
struct Product {
	ean: u64,
	name: String,
}

#[derive(Deserialize,Serialize, zbus::zvariant::Type)]
struct InvoiceEntry {
	timestamp: i64,
	product: Product,
	price: i32,
}

#[derive(Deserialize,Serialize, zbus::zvariant::Type)]
struct UserSaleStatsEntry {
    timedatecode: String,
    count: i32,
}

#[derive(Deserialize,Serialize, zbus::zvariant::Type)]
struct Category {
	id: i32,
	name: String,
}

#[derive(Deserialize,Serialize, zbus::zvariant::Type)]
struct Supplier {
	id: i64,
	name: String,
	postal_code: String,
	city: String,
	street: String,
	phone: String,
	website: String,
}

#[derive(Deserialize,Serialize, zbus::zvariant::Type)]
struct EanAlias {
	ean: u64,
	real_ean: u64,
}

#[derive(Deserialize,Serialize, zbus::zvariant::Type)]
struct UserAuth {
	id: i32,
	superuser: bool,
	auth_cashbox: bool,
	auth_products: bool,
	auth_users: bool,
}

#[derive(Deserialize,Serialize, zbus::zvariant::Type)]
struct UserInfo {
	id: i32,
	firstname: String,
	lastname: String,
	email: String,
	gender: String,
	street: String,
	postcode: String,
	city: String,
	pgp: String,
	joined_at: i64,
	disabled: bool,
	hidden: bool,
	sound_theme: String,
	rfid: Vec<String>,
}

impl UserInfo {
    fn equals(&self, x: &UserInfo ) -> bool {
		if self.id != x.id {  return false; }
		if self.firstname != x.firstname {  return false; }
		if self.lastname != x.lastname {  return false; }
		if self.email != x.email {  return false; }
		if self.gender != x.gender {  return false; }
		if self.street != x.street {  return false; }
		if self.postcode != x.postcode {  return false; }
		if self.city != x.city {  return false; }
		if self.pgp != x.pgp {  return false; }
		if self.joined_at != x.joined_at {  return false; }
		if self.disabled != x.disabled {  return false; }
		if self.hidden != x.hidden {  return false; }

		/* check if both objects contain the same RFIDs */
        for id in &self.rfid {
			if !x.rfid.contains(&id) {
				return false;
            }
		}

		for id in &x.rfid {
			if !self.rfid.contains(&id) {
				return false;
            }
		}

        true
    }
}

#[derive(Deserialize,Serialize, zbus::zvariant::Type)]
struct BestBeforeEntry {
	ean: u64,
	name: String,
	amount: i32,
	best_before_date: i64,
}

#[derive(Deserialize,Serialize, zbus::zvariant::Type)]
struct CashboxDiff {
	user: i32,
	amount: i32,
	timestamp: i64,
}

#[derive(Deserialize,Serialize, zbus::zvariant::Type)]
struct ProductMetadata {
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

fn get_unix_time() -> i64 {
    match std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH) {
        Ok(n) => n.as_secs().try_into().expect("Cannot convert timestamp from u64 to i64"),
        Err(_) => panic!("SystemTime before UNIX EPOCH!"),
    }
}

fn sha256(msg: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(msg);
    hasher.finalize().encode_hex::<String>()
}

#[interface(name = "io.mainframe.shopsystem.Database")]
impl Database {

	fn get_products(&mut self) -> Result<HashMap<u64,String>, DatabaseError> {
		let mut result = HashMap::new();
        let query = "SELECT id, name FROM products ORDER BY id";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let mut rows = statement.query([])?;

        while let Some(row) = rows.next()? {
            result.insert(row.get(0)?, row.get(1)?);
        }

		Ok(result)
	}

	fn products_search(&mut self, search_query: &str) -> Result<Vec<Product>, DatabaseError> {
		let mut result = Vec::new();
        let query = "SELECT id, name FROM products WHERE name LIKE '%' || ? || '%' ORDER BY id";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let mut rows = statement.query([search_query])?;

        while let Some(row) = rows.next()? {
            result.push(Product {
                ean: row.get(0)?,
                name: row.get(1)?
            });
        }

		Ok(result)
    }

    fn get_productlist(&mut self) -> Result<Vec<DetailedProductInfo>, DatabaseError> {
		let mut result = Vec::new();
        let query = "SELECT products.id, products.name, categories.name, amount, memberprice, guestprice, deprecated FROM products, prices, categories WHERE products.id = prices.product AND categories.id = products.category AND prices.valid_from = (SELECT valid_from FROM prices WHERE product = products.id ORDER BY valid_from DESC LIMIT 1) ORDER BY categories.name, products.name";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let mut rows = statement.query([])?;

        while let Some(row) = rows.next()? {
            let id = row.get(0)?;

            result.push(DetailedProductInfo {
                ean: id,
                aliases: self.get_product_aliases(id)?,
                name: row.get(1)?,
                category: row.get(2)?,
                amount: row.get(3)?,
                memberprice: row.get(4)?,
                guestprice: row.get(5)?,
                deprecated: row.get(6)?,
            });
        }

        Ok(result)
    }

    fn get_stock(&mut self) -> Result<Vec<DetailedProduct>, DatabaseError> {
		let mut result = Vec::new();
        let query = "SELECT stock.id, stock.name, categories.name, amount, memberprice, guestprice FROM stock, prices, categories WHERE stock.id = prices.product AND categories.id = stock.category AND prices.valid_from = (SELECT valid_from FROM prices WHERE product = stock.id ORDER BY valid_from DESC LIMIT 1) ORDER BY categories.name, stock.name";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let mut rows = statement.query([])?;

        while let Some(row) = rows.next()? {
            result.push(DetailedProduct {
                ean: row.get(0)?,
                name: row.get(1)?,
                category: row.get(2)?,
                amount: row.get(3)?,
                memberprice: row.get(4)?,
                guestprice: row.get(5)?,
            });
        }

        Ok(result)
    }

	fn get_product_for_ean(&mut self, ean: u64) -> Result<DetailedProduct, DatabaseError> {
        let ean = self.ean_alias_get(ean)?;
		let result = DetailedProduct {
            ean: ean,
            name: self.get_product_name(ean)?,
            category: self.get_product_category(ean)?,
            amount: self.get_product_amount(ean)?,
            memberprice: self.get_product_price(1, ean)?,
            guestprice: self.get_product_price(0, ean)?,
        };
        Ok(result)
	}

	fn product_metadata_set(&mut self, ean: u64, metadata: ProductMetadata) -> Result<(), DatabaseError> {
        let ean = self.ean_alias_get(ean)?;
        let connection = self.pool.get()?;
        let query = "INSERT OR REPLACE INTO product_metadata ('product', 'product_size', 'product_size_is_weight', 'container_size', 'calories', 'carbohydrates', 'fats', 'proteins', 'deposit', 'container_deposit') VALUES (?,?,?,?,?,?,?,?,?,?)";
        let mut statement = connection.prepare(query)?;
        let _inserted_row_count = statement.execute((
            ean,
            metadata.product_size,
            metadata.product_size_is_weight,
            metadata.container_size,
            metadata.calories,
            metadata.carbohydrates,
            metadata.fats,
            metadata.proteins,
            metadata.deposit,
            metadata.container_deposit,
        ))?;

        Ok(())
	}

	fn product_metadata_get(&mut self, ean: u64) -> Result<ProductMetadata, DatabaseError> {
        let query = "SELECT product_size, product_size_is_weight, container_size, calories, carbohydrates, fats, proteins, deposit, container_deposit FROM product_metadata WHERE product = ?";
        let ean = self.ean_alias_get(ean)?;
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;

        let result = statement.query_row([ean], |r| {
            Ok(ProductMetadata {
                product_size: r.get(0)?,
                product_size_is_weight: r.get(1)?,
                container_size: r.get(2)?,
                calories: r.get(3)?,
                carbohydrates: r.get(4)?,
                fats: r.get(5)?,
                proteins: r.get(6)?,
                deposit: r.get(7)?,
                container_deposit: r.get(8)?,
            })
        })?;
        Ok(result)
	}

	fn get_product_sales_info(&mut self, ean: u64, since: i64) -> Result<u32, DatabaseError> {
        let query = "select COUNT(*) AS sold_items from sales where sales.product = ? and datetime(timestamp, 'unixepoch', 'localtime') > datetime(?, 'unixepoch', 'localtime')";
        let ean = self.ean_alias_get(ean)?;
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;

        let result = statement.query_row((ean, since), |r| { Ok(r.get(0)?) })?;
        Ok(result)
	}

	fn get_prices(&mut self, product: u64) -> Result<Vec<PriceEntry>, DatabaseError> {
		let mut result = Vec::new();
        let query = "SELECT valid_from, memberprice, guestprice FROM prices WHERE product = ? ORDER BY valid_from ASC;";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let mut rows = statement.query([product])?;

        while let Some(row) = rows.next()? {
            result.push(PriceEntry {
                valid_from: row.get(0)?,
                memberprice: row.get(1)?,
                guestprice: row.get(2)?,
            });
        }

		Ok(result)
	}

	fn get_restocks(&mut self, product: u64, descending: bool) -> Result<Vec<RestockEntry>, DatabaseError> {
		let mut result = Vec::new();
        let query_asc = "SELECT timestamp, amount, price, supplier, best_before_date FROM restock WHERE product = ? ORDER BY timestamp ASC;";
        let query_desc = "SELECT timestamp, amount, price, supplier, best_before_date FROM restock WHERE product = ? ORDER BY timestamp DESC;";
        let query = match descending {
            true => query_desc,
            false => query_asc,
        };
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let mut rows = statement.query([product])?;

        while let Some(row) = rows.next()? {
            result.push(RestockEntry {
                timestamp: row.get(0)?,
                amount: row.get(1)?,
                price: row.get(2)?,
                supplier: row.get(3).unwrap_or(0),
                best_before_date: row.get(4).unwrap_or(0),
            });
        }

		Ok(result)
	}

	fn get_last_restock(&mut self, product: u64, min_price: u32) -> Result<RestockEntry, DatabaseError> {
        let query = "SELECT timestamp, amount, price, supplier, best_before_date FROM restock WHERE product = ? AND price >= ? ORDER BY timestamp DESC LIMIT 1;";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let (timestamp, amount, price, supplier, bbd) = statement.query_row((product, min_price),
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?;

        Ok(RestockEntry {
            timestamp: timestamp,
            amount: amount,
            price: price,
            supplier: supplier,
            best_before_date: bbd,
        })
    }

	fn buy(&mut self, user: i32, article: u64) -> Result<(), DatabaseError> {
        let query = "INSERT INTO sales ('user', 'product', 'timestamp') VALUES (?, ?, ?)";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let timestamp = get_unix_time();
        let _inserted_row_count = statement.execute((user, article, timestamp))?;
        Ok(())
	}

	fn get_product_name(&mut self, article: u64) -> Result<String, DatabaseError> {
        let query = "SELECT name FROM products WHERE id = ?";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let name = statement.query_row([article], |r| r.get(0))?;
        Ok(name)
	}

	fn get_product_aliases(&mut self, article: u64) -> Result<Vec<u64>, DatabaseError> {
		let mut result = Vec::new();
        let query = "SELECT id FROM ean_aliases WHERE real_ean = ?";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let mut rows = statement.query([article])?;

        while let Some(row) = rows.next()? {
            result.push(row.get(0)?);
        }

		Ok(result)
	}

	fn get_product_category(&mut self, article: u64) -> Result<String, DatabaseError> {
        let query = "SELECT categories.name FROM categories, products WHERE products.category = categories.id AND products.id = ?";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let category = statement.query_row([article], |r| r.get(0))?;
        Ok(category)
	}

	fn get_product_amount(&mut self, article: u64) -> Result<i32, DatabaseError> {
        let query = "SELECT amount FROM products WHERE id = ?";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let amount = statement.query_row([article], |r| r.get(0))?;
        Ok(amount)
    }

	fn get_product_amount_with_container_size(&mut self, article: u64) -> Result<(i32, u32), DatabaseError> {
        let amount = self.get_product_amount(article)?;

        let query = "SELECT container_size FROM product_metadata WHERE product = ?";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;

        let container_size = match statement.query_one([article], |row| row.get(0)).optional()? {
            Some(val) => val,
            None => 0,
        };

        Ok((amount, container_size))
    }

	fn get_product_deprecated(&mut self, article: u64) -> Result<bool, DatabaseError> {
        let query = "SELECT deprecated FROM products WHERE id = ?";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let deprecated = statement.query_row([article], |r| r.get(0))?;
        Ok(deprecated)
	}

	fn product_deprecate(&mut self, article: u64, value: bool) -> Result<(), DatabaseError> {
        let query = "UPDATE products SET deprecated=? WHERE id = ?";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let _inserted_row_count = statement.execute((value, article))?;
        Ok(())
	}

	fn get_product_price(&mut self, user: i32, article: u64) -> Result<i32, DatabaseError> {
        let timestamp = get_unix_time().try_into().unwrap();
        let member = user != 0;
        let query = "SELECT memberprice, guestprice FROM prices WHERE product = ? AND valid_from <= ? ORDER BY valid_from DESC LIMIT 1";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let (memberprice, guestprice) = statement.query_row([article, timestamp], |r| Ok((r.get(0)?, r.get(1)?)))?;

        if member {
            Ok(memberprice)
        } else {
            Ok(guestprice)
        }
    }

	fn ean_alias_get(&mut self, ean: u64) -> Result<u64, DatabaseError> {
        let query = "SELECT real_ean FROM ean_aliases WHERE id = ? UNION ALL SELECT ? LIMIT 1";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let result = statement.query_row([ean, ean], |r| r.get(0))?;
        Ok(result)
	}

	fn undo(&mut self, user: i32) -> Result<String, DatabaseError> {
        let query_undo_info = "SELECT product FROM sales WHERE user = ? ORDER BY timestamp DESC LIMIT 1";
        let query_undo = "DELETE FROM sales WHERE user = ? ORDER BY timestamp DESC LIMIT 1";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query_undo_info)?;
        let pid = statement.query_row([user], |r| r.get(0))?;
        let pname = self.get_product_name(pid)?;
        let mut statement = connection.prepare(query_undo)?;
        let _inserted_row_count = statement.execute([user])?;
        Ok(pname)
	}

	fn get_category_list(&mut self) -> Result<Vec<Category>, DatabaseError> {
		let mut result = Vec::new();
        let query = "SELECT id, name FROM categories";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let mut rows = statement.query([])?;

        while let Some(row) = rows.next()? {
            result.push(Category {
                id: row.get(0)?,
                name: row.get(1)?,
            });
        }

		Ok(result)
	}

	fn restock(&mut self, user: i32, product: u64, amount: u32, price: u32, supplier: i32, best_before_date: i64) -> Result<(), DatabaseError> {
        let timestamp = get_unix_time();
        let query = "INSERT INTO restock ('user', 'product', 'amount', 'price', 'timestamp', 'supplier', 'best_before_date') VALUES (?, ?, ?, ?, ?, ?, ?)";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let _inserted_row_count = statement.execute((user, product, amount, price, timestamp, supplier, best_before_date))?;
        Ok(())
	}

	fn new_product(&mut self, ean: u64, name: &str, category: i32, memberprice: i32, guestprice: i32) -> Result<(), DatabaseError> {
        let query = "INSERT INTO products ('id', 'name', 'category', 'amount') VALUES (?, ?, ?, ?)";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let _inserted_row_count = statement.execute((ean, name, category, 0))?;
		self.new_price(ean, 0, memberprice, guestprice)?;
        Ok(())
	}

	fn new_price(&mut self, product: u64, timestamp: i64, memberprice: i32, guestprice: i32) -> Result<(), DatabaseError> {
        let query = "INSERT INTO prices ('product', 'valid_from', 'memberprice', 'guestprice') VALUES (?, ?, ?, ?)";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let _inserted_row_count = statement.execute((product, timestamp, memberprice, guestprice))?;
        Ok(())
	}

	fn check_user_password(&mut self, user: i32, password: &str) -> Result<bool, DatabaseError> {
        let query = "SELECT password FROM authentication WHERE user = ?";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let pwhash_db: String = match statement.query_row([user], |r| r.get(0)) {
            Ok(password) => password,
            Err(error) => {
                return match error {
                    r2d2_sqlite::rusqlite::Error::QueryReturnedNoRows => Ok(false),
                    _ => Err(error.into()),
                };
            }
        };
        let pwhash_user = sha256(password);
        Ok(pwhash_db == pwhash_user)
	}

	fn get_supplier_list(&mut self) -> Result<Vec<Supplier>, DatabaseError> {
		let mut result = Vec::new();
        let query = "SELECT id, name, postal_code, city, street, phone, website FROM supplier";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let mut rows = statement.query([])?;

        while let Some(row) = rows.next()? {
            result.push(Supplier {
                id: row.get(0)?,
                name: row.get(1)?,
                postal_code: row.get(2).unwrap_or("".to_string()),
                city: row.get(3).unwrap_or("".to_string()),
                street: row.get(4).unwrap_or("".to_string()),
                phone: row.get(5).unwrap_or("".to_string()),
                website: row.get(6).unwrap_or("".to_string()),
            });
        }

		Ok(result)
	}

	fn get_supplier_product_list(&mut self, supplier: i32) -> Result<Vec<ProductInfo>, DatabaseError> {
		let mut result = Vec::new();
        let query = "select products.id,products.name from restock,products where products.id = restock.product and supplier = ? and deprecated = false and strftime('%Y-%m-%d', timestamp, 'unixepoch', '+1 years') > strftime('%Y-%m-%d') group by products.id";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let mut rows = statement.query([supplier])?;

        while let Some(row) = rows.next()? {
            result.push(ProductInfo {
                ean: row.get(0)?,
                name: row.get(1)?,
            });
        }

		Ok(result)
	}

	fn get_supplier_restock_dates(&mut self, supplier: i32) -> Result<Vec<i64>, DatabaseError> {
		let mut result = Vec::new();
        let query = "select unixepoch(strftime('%Y-%m-%d', timestamp, 'unixepoch', 'localtime')) from restock where supplier = ? group by strftime('%Y-%m-%d', timestamp, 'unixepoch', 'localtime') order by timestamp desc limit 10";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let mut rows = statement.query([supplier])?;

        while let Some(row) = rows.next()? {
            result.push(row.get(0)?);
        }

		Ok(result)
	}

	fn ean_alias_list(&mut self) -> Result<Vec<EanAlias>, DatabaseError> {
		let mut result = Vec::new();
        let query = "SELECT id, real_ean FROM ean_aliases ORDER BY id ASC";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let mut rows = statement.query([])?;

        while let Some(row) = rows.next()? {
            result.push(EanAlias {
                ean: row.get(0)?,
                real_ean: row.get(1)?,
            });
        }

		Ok(result)
	}

	fn get_supplier(&mut self, id: i32) -> Result<Supplier, DatabaseError> {
        let query = "SELECT id, name, postal_code, city, street, phone, website FROM supplier WHERE id = ?";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let response = statement.query_row([id], |row| Ok(Supplier {
            id: row.get(0)?,
            name: row.get(1)?,
            postal_code: row.get(2).unwrap_or("".to_string()),
            city: row.get(3).unwrap_or("".to_string()),
            street: row.get(4).unwrap_or("".to_string()),
            phone: row.get(5).unwrap_or("".to_string()),
            website: row.get(6).unwrap_or("".to_string()),
        }));
        match response {
            Ok(supplier) => Ok(supplier),
            Err(err) => {
                match err {
                    r2d2_sqlite::rusqlite::Error::QueryReturnedNoRows => Ok(Supplier {
                        id: 0,
                        name: "Unknown".to_string(),
                        postal_code: "".to_string(),
                        city: "".to_string(),
                        street: "".to_string(),
                        phone: "".to_string(),
                        website: "".to_string(),
                    }),
                    _ => Err(err.into()),
                }
            }
        }
	}

	fn set_user_password(&mut self, user: i32, password: &str) -> Result<(), DatabaseError> {
        let pwhash = sha256(password);
        let connection = self.pool.get()?;

        let query_auth_create = "INSERT OR IGNORE INTO authentication (user) VALUES (?)";
        let query_password_set = "UPDATE authentication SET password = ? WHERE user = ?";

        let mut statement = connection.prepare(query_auth_create)?;
        let _inserted_row_count = statement.execute([user])?;

        let mut statement = connection.prepare(query_password_set)?;
        let _inserted_row_count = statement.execute((pwhash, user))?;
        Ok(())
    }

    fn set_sessionid(&mut self, user: i32, sessionid: &str) -> Result<() , DatabaseError> {
        let query = "UPDATE authentication SET session=? WHERE user = ?";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let _inserted_row_count = statement.execute((sessionid, user))?;
        Ok(())
    }

    #[allow(non_snake_case)]
    fn set_userTheme(&mut self, user: i32, user_theme: &str) -> Result<() , DatabaseError> {
        let query = "UPDATE users SET sound_theme=? WHERE id = ?";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let theme = if user_theme == "" { None } else { Some(user_theme) };
        let _inserted_row_count = statement.execute((theme, user))?;
        Ok(())
    }

    fn get_user_by_sessionid(&mut self, sessionid: &str) -> Result<i32, DatabaseError> {
        let query = "SELECT user FROM authentication WHERE session = ?";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let userid = statement.query_row([sessionid], |r| r.get(0))?;
        Ok(userid)
    }

    fn get_user_info(&mut self, user: i32) -> Result<UserInfo, DatabaseError> {
        let connection = self.pool.get()?;

        let query = "SELECT firstname, lastname, email, gender, street, plz, city, pgp, joined_at, disabled, hidden, sound_theme FROM users WHERE id = ?";
        let mut statement = connection.prepare(query)?;
        let mut userinfo = statement.query_row([user], |r| Ok({
            let plz: u64 = r.get(5)?;
            UserInfo {
                id: user,
                firstname: r.get(0)?,
                lastname: r.get(1)?,
                email: r.get(2)?,
                gender: r.get(3)?,
                street: r.get(4)?,
                postcode: format!("{}", plz),
                city: r.get(6)?,
                pgp: r.get(7)?,
                joined_at: r.get(8)?,
                disabled: r.get(9)?,
                hidden: r.get(10)?,
                sound_theme: r.get(11).unwrap_or("".to_string()),
                rfid: Vec::new(),
            }
        }))?;

        let rfidquery = "SELECT rfid FROM rfid_users WHERE user = ?";
        let mut statement = connection.prepare(rfidquery)?;
        let mut rows = statement.query([user])?;

        while let Some(row) = rows.next()? {
            userinfo.rfid.push(row.get(0)?);
        }

        Ok(userinfo)
    }

    fn get_user_auth(&mut self, user: i32) -> Result<UserAuth, DatabaseError> {
        let connection = self.pool.get()?;
        let query = "SELECT superuser, auth_users, auth_products, auth_cashbox FROM authentication WHERE user = ?";
        let mut statement = connection.prepare(query)?;
        let response = statement.query_row([user], |r| Ok(UserAuth {
            id: user,
            superuser: r.get(0)?,
            auth_users: r.get(1)?,
            auth_products: r.get(2)?,
            auth_cashbox: r.get(3)?,
        }));
        match response {
            Ok(userauth) => Ok(userauth),
            Err(err) => {
                match err {
                    r2d2_sqlite::rusqlite::Error::QueryReturnedNoRows => Ok(UserAuth {
                        id: user,
                        superuser: false,
                        auth_cashbox: false,
                        auth_products: false,
                        auth_users: false,
                    }),
                    _ => Err(err.into()),
                }
            }
        }
    }

    fn set_user_auth(&mut self, auth: UserAuth) -> Result<(), DatabaseError> {
        let connection = self.pool.get()?;

        let query_auth_create = "INSERT OR IGNORE INTO authentication (user) VALUES (?)";
        let mut statement = connection.prepare(query_auth_create)?;
        let _inserted_row_count = statement.execute([auth.id])?;

        let query = "UPDATE authentication SET auth_users = ?, auth_products = ?, auth_cashbox = ? WHERE user = ?";
        let mut statement = connection.prepare(query)?;
        let _inserted_row_count = statement.execute((auth.auth_users, auth.auth_products, auth.auth_cashbox, auth.id))?;
        Ok(())
    }

    fn get_username(&mut self, user: i32) -> Result<String, DatabaseError> {
        let query = "SELECT firstname, lastname FROM users WHERE id = ?";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let name = statement.query_row([user], |r| {
            let firstname: String = r.get(0)?;
            let lastname: String = r.get(1)?;
            Ok(format!("{} {}", firstname, lastname))
        })?;
        Ok(name)
    }

    fn get_user_theme(&mut self, user: i32, fallback: String) -> Result<String, DatabaseError> {
        let query = "SELECT CASE WHEN sound_theme IS NULL THEN ? ELSE sound_theme END FROM users WHERE id = ?";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let theme = statement.query_row((fallback, user), |r| r.get(0))?;
        Ok(theme)
    }

    fn get_invoice(&mut self, user: i32, from: i64, to: i64) -> Result<Vec<InvoiceEntry>, DatabaseError> {
        let query = "SELECT timestamp, id AS productid, name AS productname, CASE WHEN user < 0 THEN (SELECT SUM(price * amount) / SUM(amount) FROM restock WHERE restock.product = id AND restock.timestamp <= sales.timestamp) else (SELECT CASE WHEN user=0 THEN guestprice else memberprice END FROM prices WHERE product = id AND valid_from <= timestamp ORDER BY valid_from DESC LIMIT 1) END AS price FROM sales INNER JOIN products ON sales.product = products.id WHERE user = ? AND timestamp >= ? AND timestamp <= ? ORDER BY timestamp";
		let mut result = Vec::new();
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let to = if to < 0 { get_unix_time() } else { to };
        let mut rows = statement.query((user, from, to))?;

        while let Some(row) = rows.next()? {
            result.push(InvoiceEntry {
                timestamp: row.get(0)?,
                product: Product {
                    ean: row.get(1)?,
                    name: row.get(2)?,
                },
                price: row.get(3)?,
            });
        }

		Ok(result)
    }

    fn get_first_purchase(&mut self, user: i32) -> Result<i64, DatabaseError> {
        let query = "SELECT timestamp FROM sales WHERE user = ? ORDER BY timestamp ASC  LIMIT 1";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let response = statement.query_row([user], |r| r.get(0));
        match response {
            Ok(timestamp) => Ok(timestamp),
            Err(err) => {
                match err {
                    r2d2_sqlite::rusqlite::Error::QueryReturnedNoRows => Ok(0),
                    _ => Err(err.into()),
                }
            }
        }
    }

    fn get_last_purchase(&mut self, user: i32) -> Result<i64, DatabaseError> {
        let query = "SELECT timestamp FROM sales WHERE user = ? ORDER BY timestamp DESC LIMIT 1";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let response = statement.query_row([user], |r| r.get(0));
        match response {
            Ok(timestamp) => Ok(timestamp),
            Err(err) => {
                match err {
                    r2d2_sqlite::rusqlite::Error::QueryReturnedNoRows => Ok(0),
                    _ => Err(err.into()),
                }
            }
        }
    }

    fn get_user_sale_stats(&mut self, user: i32, timecode: &str) -> Result<Vec<UserSaleStatsEntry>, DatabaseError> {
        let query = "select strftime(?, datetime(timestamp, 'unixepoch')), COUNT(*) from sales where user = ? group by strftime(?, datetime(timestamp, 'unixepoch'));";
		let mut result = Vec::new();
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let mut rows = statement.query((timecode, user, timecode))?;

        while let Some(row) = rows.next()? {
            result.push(UserSaleStatsEntry {
                timedatecode: row.get(0)?,
                count: row.get(1)?,
            });
        }

		Ok(result)
    }

    fn get_member_ids(&mut self) -> Result<Vec<i32>, DatabaseError> {
        let query = "SELECT id FROM users WHERE id > 0";
		let mut result = Vec::new();
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let mut rows = statement.query([])?;

        while let Some(row) = rows.next()? {
            result.push(row.get(0)?);
        }

		Ok(result)
    }

    fn get_system_member_ids(&mut self) -> Result<Vec<i32>, DatabaseError> {
        let query = "SELECT id FROM users WHERE id <= 0";
		let mut result = Vec::new();
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let mut rows = statement.query([])?;

        while let Some(row) = rows.next()? {
            result.push(row.get(0)?);
        }

		Ok(result)
    }

    fn user_disable(&mut self, user: i32, value: bool) -> Result<(), DatabaseError> {
        let query = "UPDATE users SET disabled = ? WHERE id = ?";
        // revoke permissions for disabled accounts
        if value == true {
            self.set_user_auth(UserAuth {
                id: user,
                superuser: false,
                auth_users: false,
                auth_products: false,
                auth_cashbox: false,
            })?;
        }
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let _inserted_row_count = statement.execute((value, user))?;
        Ok(())
    }

    fn user_replace(&mut self, u: UserInfo) -> Result<(), DatabaseError> {
        let connection = self.pool.get()?;

        let query = "INSERT OR REPLACE INTO users ('id', 'email', 'firstname', 'lastname', 'gender', 'street', 'plz', 'city', 'pgp', 'hidden', 'disabled', 'joined_at', 'sound_theme') VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, (select sound_theme from users where id = ?))";
        let mut statement = connection.prepare(query)?;
        let _inserted_row_count = statement.execute((u.id, u.email, u.firstname, u.lastname, u.gender, u.street, u.postcode, u.city, u.pgp, u.hidden, u.disabled, u.joined_at, u.id))?;

        let query_delete_rfid = "DELETE FROM rfid_users WHERE user = ?";
        let mut statement = connection.prepare(query_delete_rfid)?;
        let _inserted_row_count = statement.execute([u.id])?;

        let query_add_rfid = "INSERT OR REPLACE INTO rfid_users ('user','rfid') VALUES (?,?)";
        let mut statement = connection.prepare(query_add_rfid)?;

        for rfid in &u.rfid {
            let _inserted_row_count = statement.execute((u.id, rfid))?;
		}

        Ok(())
    }

    fn user_is_disabled(&mut self, user: i32) -> Result<bool, DatabaseError> {
        Ok(self.get_user_info(user)?.disabled)
    }

    fn user_exists(&mut self, user: i32) -> Result<bool, DatabaseError> {
		Ok(if self.get_member_ids()?.contains(&user) { true } else { false })
    }

    fn user_equals(&mut self, u: UserInfo) -> Result<bool, DatabaseError> {
		let dbu = self.get_user_info(u.id)?;
		Ok(u.equals(&dbu))
    }

    fn get_timestamp_of_last_purchase(&mut self) -> Result<i64, DatabaseError> {
        let query = "SELECT timestamp FROM sales ORDER BY timestamp DESC LIMIT 1";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let response = statement.query_row([], |r| r.get(0));
        match response {
            Ok(timestamp) => Ok(timestamp),
            Err(err) => {
                match err {
                    r2d2_sqlite::rusqlite::Error::QueryReturnedNoRows => Ok(0),
                    _ => Err(err.into()),
                }
            }
        }
    }

    fn add_category(&mut self, name: String) -> Result<(), DatabaseError> {
		/* check if category already exists */
        for c in self.get_category_list()? {
			if name == c.name {
				return Ok(());
			}
		}

        let query = "INSERT INTO categories('name') VALUES (?)";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let _inserted_row_count = statement.execute([name])?;
        Ok(())
    }

    fn add_supplier(&mut self, name: String, postal_code: String, city: String, street: String, phone: String, website: String) -> Result<(), DatabaseError> {
        let query = "INSERT INTO supplier('name', 'postal_code', 'city', 'street', 'phone', 'website') VALUES (?, ?, ?, ?, ?, ?)";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let _inserted_row_count = statement.execute((name, postal_code, city, street, phone, website))?;
        Ok(())
    }

    fn get_users_with_sales(&mut self, timestamp_from: i64, timestamp_to: i64) -> Result<Vec<i32>, DatabaseError> {
        let query = "SELECT user FROM sales WHERE timestamp > ? AND timestamp < ? GROUP BY user";
		let mut result = Vec::new();
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let mut rows = statement.query([timestamp_from, timestamp_to])?;

        while let Some(row) = rows.next()? {
            result.push(row.get(0)?);
        }

		Ok(result)
    }

    fn get_user_invoice_sum(&mut self, user: i32, timestamp_from: i64, timestamp_to: i64) -> Result<i32, DatabaseError> {
        let query = "SELECT SUM(CASE WHEN user < 0 THEN (SELECT SUM(price * amount) / SUM(amount) FROM restock WHERE restock.product = id AND restock.timestamp <= sales.timestamp) else (SELECT CASE WHEN user=0 THEN guestprice else memberprice END FROM prices WHERE product = id AND valid_from <= timestamp ORDER BY valid_from DESC LIMIT 1) END) FROM sales INNER JOIN products ON sales.product = products.id WHERE user = ? AND timestamp >= ? AND timestamp <= ? ORDER BY timestamp";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let response = statement.query_row((user, timestamp_from, timestamp_to), |r| r.get(0));
        match response {
            Ok(price) => Ok(price),
            Err(err) => {
                match err {
                    r2d2_sqlite::rusqlite::Error::QueryReturnedNoRows => Ok(0),
                    _ => Err(err.into()),
                }
            }
        }
    }

    fn cashbox_status(&mut self) -> Result<i32, DatabaseError> {
        let query = "SELECT amount FROM current_cashbox_status";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let response = statement.query_row([], |r| r.get(0));
        match response {
            Ok(price) => Ok(price),
            Err(err) => {
                match err {
                    r2d2_sqlite::rusqlite::Error::QueryReturnedNoRows => Ok(0),
                    _ => Err(err.into()),
                }
            }
        }
    }

    fn cashbox_add(&mut self, user: i32, amount: i32, timestamp: i64) -> Result<(), DatabaseError> {
        let query = "INSERT INTO cashbox_diff ('user', 'amount', 'timestamp') VALUES (?, ?, ?)";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let _inserted_row_count = statement.execute((user, amount, timestamp))?;
        Ok(())
    }

    fn cashbox_history(&mut self) -> Result<Vec<CashboxDiff>, DatabaseError> {
        let query = "SELECT user, amount, timestamp FROM cashbox_diff ORDER BY timestamp DESC LIMIT 10";
		let mut result = Vec::new();
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let mut rows = statement.query([])?;

        while let Some(row) = rows.next()? {
            result.push(CashboxDiff {
                user: row.get(0)?,
                amount: row.get(1)?,
                timestamp: row.get(2)?,
            });
        }

		Ok(result)
    }

    fn cashbox_changes(&mut self, start: i64, stop: i64) -> Result<Vec<CashboxDiff>, DatabaseError> {
        let query = "SELECT user, amount, timestamp FROM cashbox_diff WHERE timestamp >= ? and timestamp < ? ORDER BY timestamp ASC";
		let mut result = Vec::new();
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let mut rows = statement.query([start, stop])?;

        while let Some(row) = rows.next()? {
            result.push(CashboxDiff {
                user: row.get(0)?,
                amount: row.get(1)?,
                timestamp: row.get(2)?,
            });
        }

		Ok(result)
    }

    fn ean_alias_add(&mut self, ean: u64, real_ean: u64) -> Result<(), DatabaseError> {
        let query = "INSERT OR IGNORE INTO ean_aliases (id, real_ean) VALUES (?, ?)";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let _inserted_row_count = statement.execute([ean, real_ean])?;
        Ok(())
    }

    fn bestbeforelist(&mut self, ) -> Result<Vec<BestBeforeEntry>, DatabaseError> {
        let mut bbdlist = Vec::new();

		for product in self.get_stock()? {
			let mut amount = product.amount;
			let pid = product.ean;

			if amount <= 0 {
				continue;
            }

			for restock in self.get_restocks(pid, true)? {
				if (restock.amount as i32) > amount {
					bbdlist.push(BestBeforeEntry {
                        ean: pid,
                        name: product.name.clone(),
                        amount: amount,
                        best_before_date: restock.best_before_date,
                    });
				} else {
					bbdlist.push(BestBeforeEntry {
                        ean: pid,
                        name: product.name.clone(),
                        amount: restock.amount as i32,
                        best_before_date: restock.best_before_date,
                    });
				}

				amount -= restock.amount as i32;
				if amount <= 0 {
					break;
                }
			}
		}

        bbdlist.sort_by(|a, b| b.best_before_date.cmp(&a.best_before_date));
		Ok(bbdlist)
    }

    fn get_userid_for_rfid(&mut self, rfid: String) -> Result<i32, DatabaseError> {
        let query = "SELECT user FROM rfid_users WHERE rfid = ?";
        let connection = self.pool.get()?;
        let mut statement = connection.prepare(query)?;
        let user = statement.query_row([rfid], |r| r.get(0))?;
        Ok(user)
    }

}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut cfg = Ini::new();
    cfg.load("/etc/shopsystem/config.ini").expect("failed to load config");
    let dbfile = cfg.get("DATABASE", "file").expect("config does not specify DATABASE file");

    let manager = SqliteConnectionManager::file(dbfile);
    let pool = r2d2::Pool::new(manager)?;

    let db = Database {
        pool: pool,
    };

    let _connection = connection::Builder::system()?
        .name("io.mainframe.shopsystem.Database")?
        .serve_at("/io/mainframe/shopsystem/database", db)?
        .build()
        .await?;

    pending::<()>().await;

    Ok(())
}
