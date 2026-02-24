BEGIN TRANSACTION;

-- drop triggers and views using 'sales' table
DROP TRIGGER update_product_amount_on_sales_insert;
DROP TRIGGER update_product_amount_on_sales_delete;
DROP TRIGGER update_product_amount_on_sales_update;
DROP VIEW invoice;
DROP VIEW current_cashbox_status;

-- add ID and price columns
CREATE TABLE IF NOT EXISTS new_sales (id INTEGER PRIMARY KEY AUTOINCREMENT, user INTEGER NOT NULL REFERENCES users, product INTEGER NOT NULL REFERENCES products, timestamp INTEGER NOT NULL DEFAULT 0, price INTEGER NOT NULL DEFAULT 0);
INSERT INTO new_sales(user, product, timestamp) SELECT user, product, timestamp FROM sales;
DROP table sales;
ALTER TABLE new_sales RENAME TO sales;

-- update price column for normal users and guests
UPDATE sales SET price = calc.price FROM (SELECT s.id, (SELECT guestprice FROM prices p WHERE p.product = s.product AND p.valid_from <= s.timestamp ORDER BY p.valid_from DESC LIMIT 1) AS price FROM sales s WHERE user = 0) calc WHERE sales.id = calc.id AND calc.price > 0;
UPDATE sales SET price = calc.price FROM (SELECT s.id, (SELECT memberprice FROM prices p WHERE p.product = s.product AND p.valid_from <= s.timestamp ORDER BY p.valid_from DESC LIMIT 1) AS price FROM sales s WHERE user > 0) calc WHERE sales.id = calc.id AND calc.price > 0;
UPDATE sales SET price = calc.price FROM (SELECT s.id, (SELECT SUM(price * amount) / SUM(amount) FROM restock r WHERE r.product = s.product AND r.timestamp <= s.timestamp) AS price FROM sales s WHERE user < 0) calc WHERE sales.id = calc.id AND calc.price > 0;

-- re-add triggers
CREATE TRIGGER IF NOT EXISTS update_product_amount_on_sales_insert AFTER INSERT ON sales BEGIN
	UPDATE products SET amount = products.amount - 1 WHERE products.id = NEW.product;
END;

CREATE TRIGGER IF NOT EXISTS update_product_amount_on_sales_delete AFTER DELETE ON sales BEGIN
	UPDATE products SET amount = products.amount + 1 WHERE products.id = OLD.product;
END;

CREATE TRIGGER IF NOT EXISTS update_product_amount_on_sales_update AFTER UPDATE ON sales BEGIN
	UPDATE products SET amount = products.amount + 1 WHERE products.id = OLD.product;
	UPDATE products SET amount = products.amount - 1 WHERE products.id = NEW.product;
END;

-- re-add updated views
CREATE VIEW IF NOT EXISTS invoice AS SELECT user, timestamp, products.id AS productid, name AS productname, price FROM sales INNER JOIN products ON sales.product = products.id ORDER BY timestamp;
CREATE VIEW IF NOT EXISTS current_cashbox_status AS SELECT ( (SELECT SUM(price) FROM sales WHERE user = 0) + (SELECT SUM(amount) FROM cashbox_diff) ) AS amount;

COMMIT;
