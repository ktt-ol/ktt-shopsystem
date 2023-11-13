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

#[derive(zbus::DBusError, Debug)]
#[dbus_error(prefix = "io.mainframe.shopsystem.PGP")]
enum PGPError {
    #[dbus_error(zbus_error)]
    DBusError(zbus::Error),
    GPGError(String),
    Utf8Error(String),
    CompressionError(String),
}

impl From<gpgme::Error> for PGPError {
    fn from(err: gpgme::Error) -> Self {
            Self::GPGError(err.to_string())
    }
}

impl From<core::str::Utf8Error> for PGPError {
    fn from(err: core::str::Utf8Error) -> Self {
            Self::Utf8Error(err.to_string())
    }
}

impl From<compress_tools::Error> for PGPError {
    fn from(err: compress_tools::Error) -> Self {
            Self::CompressionError(err.to_string())
    }
}

struct PGP {
    keyring: String,
}

#[zbus::dbus_interface(name = "io.mainframe.shopsystem.PGP")]
impl PGP {
    fn import_archive(&mut self, data: Vec<u8>) -> Result<Vec<String>, PGPError> {
        let mut ctx = gpgme::Context::from_protocol(gpgme::Protocol::OpenPgp)?;
        ctx.set_engine_home_dir(&self.keyring)?;
        ctx.set_armor(true);
        let archivecopy = std::io::Cursor::new(&data);
        let mut archive = std::io::Cursor::new(&data);
        let mut result = Vec::new();

        for filename in compress_tools::list_archive_files(archivecopy)? {
            let mut buffer = Vec::default();
            compress_tools::uncompress_archive_file(&mut archive, &mut buffer, &filename)?;

            let bufferstr = std::str::from_utf8(&buffer).unwrap_or("");

            if !bufferstr.starts_with("-----BEGIN PGP PUBLIC KEY BLOCK-----") {
                continue;
            }

            let mut data = gpgme::Data::from_buffer(&buffer)?;
            data.set_encoding(gpgme::data::Encoding::Armor)?;
            let importresult = ctx.import(&mut data)?;

            for import in importresult.imports() {
                let fingerprint = import.fingerprint().unwrap_or("");

                if !import.status().contains(gpgme::ImportFlags::NEW) || fingerprint == "" {
                    continue;
                }

                result.push(fingerprint.to_string());
            }
        }

        Ok(result)
    }

    fn list_keys(&mut self) -> Result<Vec<String>, PGPError> {
        let mut ctx = gpgme::Context::from_protocol(gpgme::Protocol::OpenPgp)?;
        ctx.set_engine_home_dir(&self.keyring)?;
        ctx.set_armor(true);

        let mut result = Vec::new();

        let mut mode = gpgme::KeyListMode::empty();
        mode.insert(gpgme::KeyListMode::LOCAL);
        ctx.set_key_list_mode(mode)?;
        let args: Vec<String> = Vec::new();
        let mut keys = ctx.find_keys(args)?;
        for key in keys.by_ref().filter_map(|x| x.ok()) {
            let fp = key.fingerprint().unwrap_or("?");
            if fp != "?" {
                result.push(fp.to_string());
            }
        }

        Ok(result)
    }

    fn get_key(&mut self, fingerprint: String) -> Result<String, PGPError> {
        let mut ctx = gpgme::Context::from_protocol(gpgme::Protocol::OpenPgp)?;
        ctx.set_engine_home_dir(&self.keyring)?;
        ctx.set_armor(true);
        let mode = gpgme::ExportMode::empty();

        let mut args = Vec::new();
        args.push(fingerprint);

        let keys = {
            let mut key_iter = ctx.find_keys(args)?;
            let keys: Vec<_> = key_iter.by_ref().collect::<Result<_, _>>()?;
            for key in &keys {
                println!(
                    "keyid: {}  (fpr: {})",
                    key.id().unwrap_or("?"),
                    key.fingerprint().unwrap_or("?")
                );
            }
            if key_iter.finish()?.is_truncated() {
                Err(PGPError::GPGError("key listing unexpectedly truncated".to_string()))?;
            }
            keys
        };

        let mut output = Vec::new();
        ctx.export_keys(&keys, mode, &mut output)?;

        let result = std::str::from_utf8(&output)?;

        Ok(result.to_owned())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut cfg = configparser::ini::Ini::new();
    cfg.load("/etc/shopsystem/config.ini").expect("failed to load config");
    let keyring = cfg.get("PGP", "keyring").expect("config does not specify PGP keyring");

    gpgme::init();
    let pgp = PGP { keyring: keyring };

    let _connection = zbus::ConnectionBuilder::system()?
        .name("io.mainframe.shopsystem.PGP")?
        .serve_at("/io/mainframe/shopsystem/pgp", pgp)?
        .build()
        .await?;

    std::future::pending::<()>().await;

    Ok(())
}
