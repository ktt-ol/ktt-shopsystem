BEGIN TRANSACTION;
CREATE VIEW IF NOT EXISTS stock AS SELECT id, name, category, amount FROM products WHERE deprecated = 0 OR amount != 0;
CREATE VIEW IF NOT EXISTS purchaseprices AS SELECT product, SUM(price * amount) / SUM(amount) AS price FROM restock GROUP BY product;
CREATE VIEW IF NOT EXISTS invoice AS SELECT user, timestamp, products.id AS productid, name AS productname, price FROM sales INNER JOIN products ON sales.product = products.id ORDER BY timestamp;
CREATE VIEW IF NOT EXISTS current_cashbox_status AS SELECT ( (SELECT SUM(price) FROM sales WHERE user = 0) + (SELECT SUM(amount) FROM cashbox_diff) ) AS amount;
COMMIT;
