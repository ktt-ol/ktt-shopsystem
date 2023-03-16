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
use zbus::{ConnectionBuilder, DBusError, dbus_interface};
use configparser::ini::Ini;

#[derive(DBusError, Debug)]
#[dbus_error(prefix = "io.mainframe.shopsystem.Config")]
enum ConfigError {
    #[dbus_error(zbus_error)]
    ZBus(zbus::Error),
    KeyFileError,
}

struct Config {
    file: Ini,
}

#[dbus_interface(name = "io.mainframe.shopsystem.Config")]
impl Config {
	fn has_group(&mut self, group_name: &str) -> bool {
        self.file.sections().contains(&group_name.to_string())
	}

	fn has_key(&mut self, group_name: &str, key: &str) -> bool {
		self.file.get(group_name, key).is_some()
	}

	fn get_string(&mut self, group_name: &str, key: &str) -> Result<String, ConfigError> {
		self.file.get(group_name, key).ok_or(ConfigError::KeyFileError)
	}

	fn get_boolean(&mut self, group_name: &str, key: &str) -> Result<bool, ConfigError> {
		self.file.getbool(group_name, key)
            .map_err(|_| { ConfigError::KeyFileError })?
            .ok_or(ConfigError::KeyFileError)
	}

	fn get_integer(&mut self, group_name: &str, key: &str) -> Result<i32, ConfigError> {
		self.file.getint(group_name, key)
            .map_err(|_| { ConfigError::KeyFileError } )?
            .ok_or(ConfigError::KeyFileError)?
            .try_into().map_err(|_| { ConfigError::KeyFileError })
	}

	fn get_int64(&mut self, group_name: &str, key: &str) -> Result<i64, ConfigError> {
		self.file.getint(group_name, key)
            .map_err(|_| { ConfigError::KeyFileError } )?
            .ok_or(ConfigError::KeyFileError)
	}

	fn get_uint64(&mut self, group_name: &str, key: &str) -> Result<u64, ConfigError> {
		self.file.getuint(group_name, key)
            .map_err(|_| { ConfigError::KeyFileError } )?
            .ok_or(ConfigError::KeyFileError)
	}

	fn get_double(&mut self, group_name: &str, key: &str) -> Result<f64, ConfigError> {
		self.file.getfloat(group_name, key)
            .map_err(|_| { ConfigError::KeyFileError } )?
            .ok_or(ConfigError::KeyFileError)
	}
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut i = Ini::new();
    i.load("/etc/shopsystem/config.ini").unwrap();

    let cfg = Config {
        file: i,
    };

    let _connection = ConnectionBuilder::system()?
        .name("io.mainframe.shopsystem.Config")?
        .serve_at("/io/mainframe/shopsystem/config", cfg)?
        .build()
        .await?;

    pending::<()>().await;

    Ok(())
}
