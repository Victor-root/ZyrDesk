//! Everything the server remembers, in one SQLite file.
//!
//! Accounts and their hashed passwords, devices and their certificates,
//! the tokens that stand for both, contacts, shares, and a journal of
//! sessions with nothing in it but who met whom, when, and by which
//! road. Nothing here is a secret worth more than a password hash: keys
//! live elsewhere, and the media never passes through.
//!
//! One connection behind one lock. The server serves a household, not a
//! crowd, and SQLite in write-ahead mode answers in microseconds; the
//! handlers call in from a blocking thread so the runtime never waits on
//! the disk.

use std::fmt;
use std::path::Path;
use std::sync::Mutex;

use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use zyr_broker::Code;
use zyr_broker::rest::{Permission, Registration};
use zyr_broker::ticket::Grant;
use zyr_transport::Fingerprint;

/// How long a token for the gestures of the account lives.
pub const ACCOUNT_TOKEN_LIFE: u64 = 60 * 60;

/// How long a device's token lives before it is renewed.
pub const DEVICE_TOKEN_LIFE: u64 = 90 * 24 * 60 * 60;

/// With less than this left, a device's token is renewed as it connects.
pub const DEVICE_TOKEN_RENEWAL: u64 = 30 * 24 * 60 * 60;

/// Shortest password accepted. Length and nothing else: composition
/// rules make passwords worse, not better.
pub const SHORTEST_PASSWORD: usize = 12;

/// How long the journal of sessions is kept.
pub const SESSIONS_KEPT: u64 = 30 * 24 * 60 * 60;

const SCHEMA: &str = "
CREATE TABLE accounts (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password TEXT NOT NULL,
    email TEXT,
    created INTEGER NOT NULL,
    totp TEXT
);
CREATE TABLE invitations (
    code TEXT PRIMARY KEY,
    created INTEGER NOT NULL,
    used INTEGER,
    used_by TEXT REFERENCES accounts(id)
);
CREATE TABLE devices (
    id TEXT PRIMARY KEY,
    account TEXT NOT NULL REFERENCES accounts(id),
    certificate BLOB NOT NULL,
    fingerprint TEXT NOT NULL,
    name TEXT NOT NULL,
    created INTEGER NOT NULL,
    last_seen INTEGER,
    revoked INTEGER,
    signed_by TEXT,
    signature BLOB
);
CREATE UNIQUE INDEX devices_alive ON devices(fingerprint) WHERE revoked IS NULL;
CREATE INDEX devices_by_account ON devices(account);
CREATE TABLE tokens (
    hash TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    account TEXT NOT NULL REFERENCES accounts(id),
    device TEXT REFERENCES devices(id),
    created INTEGER NOT NULL,
    expires INTEGER NOT NULL
);
CREATE INDEX tokens_by_device ON tokens(device);
CREATE TABLE contacts (
    id TEXT PRIMARY KEY,
    asker TEXT NOT NULL REFERENCES accounts(id),
    asked TEXT NOT NULL REFERENCES accounts(id),
    status TEXT NOT NULL,
    created INTEGER NOT NULL,
    answered INTEGER,
    UNIQUE(asker, asked)
);
CREATE TABLE shares (
    id TEXT PRIMARY KEY,
    device TEXT NOT NULL REFERENCES devices(id),
    owner TEXT NOT NULL REFERENCES accounts(id),
    grantee TEXT NOT NULL REFERENCES accounts(id),
    permissions TEXT NOT NULL,
    expires INTEGER,
    created INTEGER NOT NULL,
    revoked INTEGER,
    signed_by TEXT,
    signature BLOB
);
CREATE INDEX shares_by_device ON shares(device);
CREATE INDEX shares_by_grantee ON shares(grantee);
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    from_device TEXT NOT NULL,
    to_device TEXT NOT NULL,
    grant TEXT NOT NULL,
    started INTEGER NOT NULL,
    ended INTEGER,
    road TEXT,
    relayed_bytes INTEGER NOT NULL DEFAULT 0
);
";

/// Why the store did not do what was asked.
#[derive(Debug)]
pub enum Fault {
    /// Refused for a reason the caller may be told.
    Refused(Code),
    /// The store itself failed; the journal gets the reason, the caller
    /// gets `Code::Internal`.
    Broken(String),
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fault::Refused(code) => write!(f, "{code:?}"),
            Fault::Broken(e) => write!(f, "base de données : {e}"),
        }
    }
}

impl std::error::Error for Fault {}

impl From<rusqlite::Error> for Fault {
    fn from(e: rusqlite::Error) -> Self {
        Fault::Broken(e.to_string())
    }
}

impl From<Code> for Fault {
    fn from(code: Code) -> Self {
        Fault::Refused(code)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub created: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub account: String,
    pub certificate: Vec<u8>,
    pub fingerprint: Fingerprint,
    pub name: String,
    pub created: u64,
    pub last_seen: Option<u64>,
    pub revoked: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    pub id: String,
    /// Account that asked.
    pub asker: String,
    /// Account that was asked.
    pub asked: String,
    pub accepted: bool,
    pub created: u64,
    pub answered: Option<u64>,
}

impl Contact {
    /// The other account, seen from this one.
    pub fn other_than<'a>(&'a self, me: &str) -> &'a str {
        if self.asker == me {
            &self.asked
        } else {
            &self.asker
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Share {
    pub id: String,
    pub device: String,
    pub owner: String,
    pub grantee: String,
    pub permissions: Vec<Permission>,
    pub expires: Option<u64>,
    pub created: u64,
    pub revoked: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invitation {
    pub code: String,
    pub created: u64,
    pub used: Option<u64>,
}

/// What `status` shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    pub accounts: u64,
    pub devices: u64,
    pub contacts: u64,
    pub shares: u64,
}

/// What the relay carried, over the sessions the base still keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Relayed {
    pub sessions: u64,
    pub bytes: u64,
}

/// A token as handed out, once: only its hash stays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub raw: String,
    pub expires: u64,
}

/// A device as read from its token, with whether that token is about to
/// run out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bearer {
    pub device: Device,
    pub account: Account,
    /// The token presented expires within the renewal window.
    pub renew: bool,
}

pub struct Store {
    conn: Mutex<Connection>,
}

/// Whether that name may be a username: three to thirty-two characters,
/// letters, digits, dot, dash, underscore.
pub fn acceptable_username(name: &str) -> bool {
    (3..=32).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
}

fn id() -> String {
    zyr_proto::random::alphanumeric_string(16)
}

fn raw_token() -> String {
    zyr_proto::random::alphanumeric_string(48)
}

fn hashed(raw: &str) -> String {
    let digest = Sha256::digest(raw.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// An invitation code as a person types it: eight characters without
/// the ones that read alike, in two groups.
fn invitation_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let picked: String = (0..8)
        .map(|_| {
            let at = rand::random_range(0..ALPHABET.len());
            char::from(ALPHABET[at])
        })
        .collect();
    format!("{}-{}", &picked[..4], &picked[4..])
}

fn hash_password(password: &str) -> Result<String, Fault> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|e| Fault::Broken(format!("hachage du mot de passe : {e}")))
}

fn password_matches(password: &str, hash: &str) -> bool {
    Argon2::default()
        .verify_password(password.as_bytes(), hash)
        .is_ok()
}

fn optional_u64(value: Option<i64>) -> Option<u64> {
    value.and_then(|v| u64::try_from(v).ok())
}

fn read_account(row: &rusqlite::Row<'_>) -> rusqlite::Result<Account> {
    Ok(Account {
        id: row.get("id")?,
        username: row.get("username")?,
        email: row.get("email")?,
        created: row.get::<_, i64>("created")? as u64,
    })
}

fn read_device(row: &rusqlite::Row<'_>) -> rusqlite::Result<Device> {
    let fingerprint: String = row.get("fingerprint")?;
    Ok(Device {
        id: row.get("id")?,
        account: row.get("account")?,
        certificate: row.get("certificate")?,
        fingerprint: fingerprint.parse().map_err(|_| {
            rusqlite::Error::InvalidColumnType(0, "fingerprint".into(), rusqlite::types::Type::Text)
        })?,
        name: row.get("name")?,
        created: row.get::<_, i64>("created")? as u64,
        last_seen: optional_u64(row.get("last_seen")?),
        revoked: optional_u64(row.get("revoked")?),
    })
}

fn read_contact(row: &rusqlite::Row<'_>) -> rusqlite::Result<Contact> {
    let status: String = row.get("status")?;
    Ok(Contact {
        id: row.get("id")?,
        asker: row.get("asker")?,
        asked: row.get("asked")?,
        accepted: status == "accepted",
        created: row.get::<_, i64>("created")? as u64,
        answered: optional_u64(row.get("answered")?),
    })
}

fn read_share(row: &rusqlite::Row<'_>) -> rusqlite::Result<Share> {
    let permissions: String = row.get("permissions")?;
    Ok(Share {
        id: row.get("id")?,
        device: row.get("device")?,
        owner: row.get("owner")?,
        grantee: row.get("grantee")?,
        permissions: serde_json::from_str(&permissions).unwrap_or_default(),
        expires: optional_u64(row.get("expires")?),
        created: row.get::<_, i64>("created")? as u64,
        revoked: optional_u64(row.get("revoked")?),
    })
}

const ACCOUNT_COLUMNS: &str = "id, username, email, created";
const DEVICE_COLUMNS: &str =
    "id, account, certificate, fingerprint, name, created, last_seen, revoked";
const CONTACT_COLUMNS: &str = "id, asker, asked, status, created, answered";
const SHARE_COLUMNS: &str = "id, device, owner, grantee, permissions, expires, created, revoked";

impl Store {
    /// Opens the file, creating it and bringing its schema up to date.
    pub fn open(path: &Path) -> Result<Self, Fault> {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)
                .map_err(|e| Fault::Broken(format!("{} : {e}", folder.display())))?;
        }
        Self::prepare(Connection::open(path)?)
    }

    /// A store that lives in memory, for the tests.
    pub fn in_memory() -> Result<Self, Fault> {
        Self::prepare(Connection::open_in_memory()?)
    }

    fn prepare(mut conn: Connection) -> Result<Self, Fault> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn with<T>(&self, work: impl FnOnce(&Connection) -> Result<T, Fault>) -> Result<T, Fault> {
        let conn = self.conn.lock().expect("connexion à la base");
        work(&conn)
    }

    /// The same, inside one transaction.
    fn within<T>(&self, work: impl FnOnce(&Connection) -> Result<T, Fault>) -> Result<T, Fault> {
        let conn = self.conn.lock().expect("connexion à la base");
        conn.execute_batch("BEGIN IMMEDIATE")?;
        match work(&conn) {
            Ok(value) => {
                conn.execute_batch("COMMIT")?;
                Ok(value)
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    // ---- Accounts --------------------------------------------------

    /// Creates an account, under the server's registration policy.
    pub fn create_account(
        &self,
        username: &str,
        password: &str,
        email: Option<&str>,
        invitation: Option<&str>,
        policy: Registration,
        now: u64,
    ) -> Result<Account, Fault> {
        if !acceptable_username(username) {
            return Err(Code::InvalidUsername.into());
        }
        if password.chars().count() < SHORTEST_PASSWORD {
            return Err(Code::WeakPassword.into());
        }
        let hash = hash_password(password)?;
        self.within(|conn| {
            let taken: bool = conn.query_row(
                "SELECT COUNT(*) > 0 FROM accounts WHERE username = ?1",
                params![username],
                |row| row.get(0),
            )?;
            if taken {
                return Err(Code::UsernameTaken.into());
            }
            match policy {
                Registration::Open => {}
                Registration::Closed => return Err(Code::RegistrationClosed.into()),
                Registration::Invitation => {
                    let code = invitation.ok_or(Code::InvitationInvalid)?;
                    let fresh: bool = conn.query_row(
                        "SELECT COUNT(*) > 0 FROM invitations WHERE code = ?1 AND used IS NULL",
                        params![code],
                        |row| row.get(0),
                    )?;
                    if !fresh {
                        return Err(Code::InvitationInvalid.into());
                    }
                }
            }
            let account = Account {
                id: id(),
                username: username.to_string(),
                email: email.map(str::to_string),
                created: now,
            };
            conn.execute(
                "INSERT INTO accounts (id, username, password, email, created) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    account.id,
                    account.username,
                    hash,
                    account.email,
                    now as i64
                ],
            )?;
            if let (Registration::Invitation, Some(code)) = (policy, invitation) {
                conn.execute(
                    "UPDATE invitations SET used = ?1, used_by = ?2 WHERE code = ?3",
                    params![now as i64, account.id, code],
                )?;
            }
            Ok(account)
        })
    }

    /// Trades a password for a token of the account.
    ///
    /// The same refusal whether the name or the password is wrong, so
    /// the answer does not say which names exist.
    pub fn login(
        &self,
        username: &str,
        password: &str,
        now: u64,
    ) -> Result<(Account, Token), Fault> {
        self.within(|conn| {
            let found = conn
                .query_row(
                    &format!(
                        "SELECT {ACCOUNT_COLUMNS}, password FROM accounts WHERE username = ?1"
                    ),
                    params![username],
                    |row| Ok((read_account(row)?, row.get::<_, String>("password")?)),
                )
                .optional()?;
            let Some((account, hash)) = found else {
                // Hashing anyway keeps the time the same whether the
                // name exists or not.
                let _ = password_matches(password, "");
                return Err(Code::InvalidCredentials.into());
            };
            if !password_matches(password, &hash) {
                return Err(Code::InvalidCredentials.into());
            }
            let token = issue_token(conn, "account", &account.id, None, now, ACCOUNT_TOKEN_LIFE)?;
            Ok((account, token))
        })
    }

    /// A token of the account, for one that was just created and has
    /// nothing to prove.
    pub fn issue_account_token(&self, account: &Account, now: u64) -> Result<Token, Fault> {
        self.with(|conn| issue_token(conn, "account", &account.id, None, now, ACCOUNT_TOKEN_LIFE))
    }

    pub fn account_of_token(&self, raw: &str, now: u64) -> Result<Account, Fault> {
        self.with(|conn| {
            conn.query_row(
                &format!(
                    "SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE id = \
                     (SELECT account FROM tokens WHERE hash = ?1 AND kind = 'account' \
                      AND expires > ?2)"
                ),
                params![hashed(raw), now as i64],
                read_account,
            )
            .optional()?
            .ok_or_else(|| Code::Unauthorized.into())
        })
    }

    #[cfg(test)]
    pub fn account_by_username(&self, username: &str) -> Result<Option<Account>, Fault> {
        self.with(|conn| {
            Ok(conn
                .query_row(
                    &format!("SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE username = ?1"),
                    params![username],
                    read_account,
                )
                .optional()?)
        })
    }

    pub fn account_by_id(&self, id: &str) -> Result<Option<Account>, Fault> {
        self.with(|conn| {
            Ok(conn
                .query_row(
                    &format!("SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE id = ?1"),
                    params![id],
                    read_account,
                )
                .optional()?)
        })
    }

    pub fn accounts(&self) -> Result<Vec<Account>, Fault> {
        self.with(|conn| {
            let mut listing = conn.prepare(&format!(
                "SELECT {ACCOUNT_COLUMNS} FROM accounts ORDER BY username"
            ))?;
            let accounts = listing
                .query_map([], read_account)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(accounts)
        })
    }

    /// Sets a new password, which is what the administrator does for a
    /// forgotten one. Every token of the account goes with the old one.
    pub fn reset_password(&self, username: &str, password: &str) -> Result<(), Fault> {
        if password.chars().count() < SHORTEST_PASSWORD {
            return Err(Code::WeakPassword.into());
        }
        let hash = hash_password(password)?;
        self.within(|conn| {
            let changed = conn.execute(
                "UPDATE accounts SET password = ?1 WHERE username = ?2",
                params![hash, username],
            )?;
            if changed == 0 {
                return Err(Code::NotFound.into());
            }
            conn.execute(
                "DELETE FROM tokens WHERE kind = 'account' AND account = \
                 (SELECT id FROM accounts WHERE username = ?1)",
                params![username],
            )?;
            Ok(())
        })
    }

    /// Removes an account and everything that was its.
    pub fn delete_account(&self, username: &str) -> Result<(), Fault> {
        self.within(|conn| {
            let account: Option<String> = conn
                .query_row(
                    "SELECT id FROM accounts WHERE username = ?1",
                    params![username],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(account) = account else {
                return Err(Code::NotFound.into());
            };
            conn.execute("DELETE FROM tokens WHERE account = ?1", params![account])?;
            conn.execute(
                "DELETE FROM shares WHERE owner = ?1 OR grantee = ?1",
                params![account],
            )?;
            conn.execute(
                "DELETE FROM contacts WHERE asker = ?1 OR asked = ?1",
                params![account],
            )?;
            conn.execute("DELETE FROM devices WHERE account = ?1", params![account])?;
            conn.execute(
                "UPDATE invitations SET used_by = NULL WHERE used_by = ?1",
                params![account],
            )?;
            conn.execute("DELETE FROM accounts WHERE id = ?1", params![account])?;
            Ok(())
        })
    }

    // ---- Invitations -----------------------------------------------

    pub fn new_invitation(&self, now: u64) -> Result<String, Fault> {
        self.with(|conn| {
            let code = invitation_code();
            conn.execute(
                "INSERT INTO invitations (code, created) VALUES (?1, ?2)",
                params![code, now as i64],
            )?;
            Ok(code)
        })
    }

    pub fn invitations(&self) -> Result<Vec<Invitation>, Fault> {
        self.with(|conn| {
            let mut listing =
                conn.prepare("SELECT code, created, used FROM invitations ORDER BY created")?;
            let invitations = listing
                .query_map([], |row| {
                    Ok(Invitation {
                        code: row.get(0)?,
                        created: row.get::<_, i64>(1)? as u64,
                        used: optional_u64(row.get(2)?),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(invitations)
        })
    }

    pub fn revoke_invitation(&self, code: &str) -> Result<(), Fault> {
        self.with(|conn| {
            let gone = conn.execute(
                "DELETE FROM invitations WHERE code = ?1 AND used IS NULL",
                params![code],
            )?;
            if gone == 0 {
                return Err(Code::NotFound.into());
            }
            Ok(())
        })
    }

    // ---- Devices ---------------------------------------------------

    /// Attaches a device to an account, and hands it its token.
    ///
    /// A device already attached to this account is attached again: same
    /// row, new name if given, new token, the old ones gone. One attached
    /// to another account moves: a device is its owner's, and the account
    /// it leaves keeps a revoked row so that its shares fall with it.
    pub fn link_device(
        &self,
        account: &str,
        certificate: &[u8],
        name: &str,
        now: u64,
    ) -> Result<(Device, Token), Fault> {
        let fingerprint = Fingerprint::of_certificate(&rustls_certificate(certificate)).to_string();
        self.within(|conn| {
            let alive = conn
                .query_row(
                    &format!(
                        "SELECT {DEVICE_COLUMNS} FROM devices WHERE fingerprint = ?1 \
                         AND revoked IS NULL"
                    ),
                    params![fingerprint],
                    read_device,
                )
                .optional()?;
            let device = match alive {
                Some(device) if device.account == account => {
                    conn.execute(
                        "UPDATE devices SET name = ?1, last_seen = ?2 WHERE id = ?3",
                        params![name, now as i64, device.id],
                    )?;
                    conn.execute("DELETE FROM tokens WHERE device = ?1", params![device.id])?;
                    Device {
                        name: name.to_string(),
                        last_seen: Some(now),
                        ..device
                    }
                }
                other => {
                    if let Some(elsewhere) = other {
                        revoke_device_rows(conn, &elsewhere.id, now)?;
                    }
                    let device = Device {
                        id: id(),
                        account: account.to_string(),
                        certificate: certificate.to_vec(),
                        fingerprint: fingerprint.parse().expect("empreinte calculée"),
                        name: name.to_string(),
                        created: now,
                        last_seen: Some(now),
                        revoked: None,
                    };
                    conn.execute(
                        "INSERT INTO devices (id, account, certificate, fingerprint, name, \
                         created, last_seen) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            device.id,
                            device.account,
                            device.certificate,
                            fingerprint,
                            device.name,
                            now as i64,
                            now as i64
                        ],
                    )?;
                    device
                }
            };
            let token = issue_token(
                conn,
                "device",
                account,
                Some(&device.id),
                now,
                DEVICE_TOKEN_LIFE,
            )?;
            Ok((device, token))
        })
    }

    /// The device behind that token, if it is still of the account.
    pub fn bearer_of_token(&self, raw: &str, now: u64) -> Result<Bearer, Fault> {
        self.with(|conn| {
            let found = conn
                .query_row(
                    "SELECT device, expires FROM tokens WHERE hash = ?1 AND kind = 'device'",
                    params![hashed(raw)],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64)),
                )
                .optional()?;
            let Some((device_id, expires)) = found else {
                return Err(Code::Unauthorized.into());
            };
            if expires <= now {
                return Err(Code::Unauthorized.into());
            }
            let device = device_by_id(conn, &device_id)?.ok_or(Code::Unauthorized)?;
            if device.revoked.is_some() {
                return Err(Code::DeviceRevoked.into());
            }
            let account = conn.query_row(
                &format!("SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE id = ?1"),
                params![device.account],
                read_account,
            )?;
            Ok(Bearer {
                device,
                account,
                renew: expires - now < DEVICE_TOKEN_RENEWAL,
            })
        })
    }

    /// A fresh token for a device whose current one is about to run out;
    /// the current one stays good until it expires.
    pub fn renew_device_token(&self, device: &Device, now: u64) -> Result<Token, Fault> {
        self.with(|conn| {
            issue_token(
                conn,
                "device",
                &device.account,
                Some(&device.id),
                now,
                DEVICE_TOKEN_LIFE,
            )
        })
    }

    pub fn device(&self, id: &str) -> Result<Option<Device>, Fault> {
        self.with(|conn| device_by_id(conn, id))
    }

    /// The living devices of an account.
    pub fn devices_of(&self, account: &str) -> Result<Vec<Device>, Fault> {
        self.with(|conn| {
            let mut listing = conn.prepare(&format!(
                "SELECT {DEVICE_COLUMNS} FROM devices WHERE account = ?1 AND revoked IS NULL \
                 ORDER BY created"
            ))?;
            let devices = listing
                .query_map(params![account], read_device)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(devices)
        })
    }

    pub fn rename_device(&self, account: &str, id: &str, name: &str) -> Result<Device, Fault> {
        self.within(|conn| {
            let changed = conn.execute(
                "UPDATE devices SET name = ?1 WHERE id = ?2 AND account = ?3 AND revoked IS NULL",
                params![name, id, account],
            )?;
            if changed == 0 {
                return Err(Code::DeviceUnknown.into());
            }
            device_by_id(conn, id)?.ok_or_else(|| Code::DeviceUnknown.into())
        })
    }

    /// Takes a device off the account: its tokens go, the shares of it go,
    /// and it may never present itself again.
    pub fn revoke_device(&self, account: &str, id: &str, now: u64) -> Result<Device, Fault> {
        self.within(|conn| {
            let device = device_by_id(conn, id)?
                .filter(|device| device.account == account && device.revoked.is_none())
                .ok_or(Code::DeviceUnknown)?;
            revoke_device_rows(conn, &device.id, now)?;
            Ok(Device {
                revoked: Some(now),
                ..device
            })
        })
    }

    pub fn touch_device(&self, id: &str, now: u64) -> Result<(), Fault> {
        self.with(|conn| {
            conn.execute(
                "UPDATE devices SET last_seen = ?1 WHERE id = ?2",
                params![now as i64, id],
            )?;
            Ok(())
        })
    }

    // ---- Contacts --------------------------------------------------

    /// Asks that account to be a contact.
    pub fn ask_contact(&self, me: &str, username: &str, now: u64) -> Result<Contact, Fault> {
        self.within(|conn| {
            let other: String = conn
                .query_row(
                    "SELECT id FROM accounts WHERE username = ?1",
                    params![username],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or(Code::NotFound)?;
            if other == me {
                return Err(Code::ContactSelf.into());
            }
            let standing = conn
                .query_row(
                    &format!(
                        "SELECT {CONTACT_COLUMNS} FROM contacts WHERE \
                         ((asker = ?1 AND asked = ?2) OR (asker = ?2 AND asked = ?1)) \
                         AND status IN ('pending', 'accepted')"
                    ),
                    params![me, other],
                    read_contact,
                )
                .optional()?;
            if standing.is_some() {
                return Err(Code::ContactExists.into());
            }
            // A request declined or a contact removed leaves its row: it
            // is asked again on the same one.
            let contact = Contact {
                id: id(),
                asker: me.to_string(),
                asked: other,
                accepted: false,
                created: now,
                answered: None,
            };
            conn.execute(
                "INSERT INTO contacts (id, asker, asked, status, created) \
                 VALUES (?1, ?2, ?3, 'pending', ?4) \
                 ON CONFLICT(asker, asked) DO UPDATE SET id = excluded.id, \
                 status = 'pending', created = excluded.created, answered = NULL",
                params![contact.id, contact.asker, contact.asked, now as i64],
            )?;
            Ok(contact)
        })
    }

    /// Accepts or declines a request made to this account.
    pub fn answer_contact(
        &self,
        me: &str,
        contact: &str,
        accept: bool,
        now: u64,
    ) -> Result<Contact, Fault> {
        self.within(|conn| {
            let status = if accept { "accepted" } else { "declined" };
            let changed = conn.execute(
                "UPDATE contacts SET status = ?1, answered = ?2 \
                 WHERE id = ?3 AND asked = ?4 AND status = 'pending'",
                params![status, now as i64, contact, me],
            )?;
            if changed == 0 {
                return Err(Code::NotFound.into());
            }
            contact_by_id(conn, contact)?.ok_or_else(|| Code::NotFound.into())
        })
    }

    /// Withdraws a request, or ends a contact, from either side; the
    /// shares between the two accounts fall with it.
    pub fn remove_contact(&self, me: &str, contact: &str, now: u64) -> Result<Contact, Fault> {
        self.within(|conn| {
            let standing = contact_by_id(conn, contact)?
                .filter(|c| c.asker == me || c.asked == me)
                .ok_or(Code::NotFound)?;
            conn.execute(
                "UPDATE contacts SET status = 'removed', answered = ?1 WHERE id = ?2",
                params![now as i64, contact],
            )?;
            conn.execute(
                "UPDATE shares SET revoked = ?1 WHERE revoked IS NULL AND \
                 ((owner = ?2 AND grantee = ?3) OR (owner = ?3 AND grantee = ?2))",
                params![now as i64, standing.asker, standing.asked],
            )?;
            Ok(standing)
        })
    }

    /// Requests and contacts of this account, pending ones included.
    pub fn contacts_of(&self, me: &str) -> Result<Vec<Contact>, Fault> {
        self.with(|conn| {
            let mut listing = conn.prepare(&format!(
                "SELECT {CONTACT_COLUMNS} FROM contacts WHERE (asker = ?1 OR asked = ?1) \
                 AND status IN ('pending', 'accepted') ORDER BY created"
            ))?;
            let contacts = listing
                .query_map(params![me], read_contact)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(contacts)
        })
    }

    // ---- Shares ----------------------------------------------------

    /// Shares one of this account's devices with a contact.
    ///
    /// A share that already stands for that device and that contact is
    /// changed in place rather than doubled.
    pub fn give_share(
        &self,
        owner: &str,
        device: &str,
        with: &str,
        permissions: &[Permission],
        expires: Option<u64>,
        now: u64,
    ) -> Result<Share, Fault> {
        self.within(|conn| {
            let owned = device_by_id(conn, device)?
                .filter(|d| d.account == owner && d.revoked.is_none())
                .ok_or(Code::ShareInvalid)?;
            let grantee: String = conn
                .query_row(
                    "SELECT id FROM accounts WHERE username = ?1",
                    params![with],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or(Code::NotFound)?;
            let friends: bool = conn.query_row(
                "SELECT COUNT(*) > 0 FROM contacts WHERE status = 'accepted' AND \
                 ((asker = ?1 AND asked = ?2) OR (asker = ?2 AND asked = ?1))",
                params![owner, grantee],
                |row| row.get(0),
            )?;
            if !friends {
                return Err(Code::NotAContact.into());
            }
            if expires.is_some_and(|until| until <= now) {
                return Err(Code::ShareInvalid.into());
            }
            let permissions_text =
                serde_json::to_string(permissions).map_err(|e| Fault::Broken(e.to_string()))?;
            let standing = conn
                .query_row(
                    &format!(
                        "SELECT {SHARE_COLUMNS} FROM shares WHERE device = ?1 AND grantee = ?2 \
                         AND revoked IS NULL"
                    ),
                    params![owned.id, grantee],
                    read_share,
                )
                .optional()?;
            let share = match standing {
                Some(share) => {
                    conn.execute(
                        "UPDATE shares SET permissions = ?1, expires = ?2 WHERE id = ?3",
                        params![permissions_text, expires.map(|e| e as i64), share.id],
                    )?;
                    Share {
                        permissions: permissions.to_vec(),
                        expires,
                        ..share
                    }
                }
                None => {
                    let share = Share {
                        id: id(),
                        device: owned.id.clone(),
                        owner: owner.to_string(),
                        grantee,
                        permissions: permissions.to_vec(),
                        expires,
                        created: now,
                        revoked: None,
                    };
                    conn.execute(
                        "INSERT INTO shares (id, device, owner, grantee, permissions, expires, \
                         created) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            share.id,
                            share.device,
                            share.owner,
                            share.grantee,
                            permissions_text,
                            expires.map(|e| e as i64),
                            now as i64
                        ],
                    )?;
                    share
                }
            };
            Ok(share)
        })
    }

    /// Takes a share back, from either side.
    pub fn remove_share(&self, me: &str, share: &str, now: u64) -> Result<Share, Fault> {
        self.within(|conn| {
            let standing = share_by_id(conn, share)?
                .filter(|s| (s.owner == me || s.grantee == me) && s.revoked.is_none())
                .ok_or(Code::NotFound)?;
            conn.execute(
                "UPDATE shares SET revoked = ?1 WHERE id = ?2",
                params![now as i64, share],
            )?;
            Ok(Share {
                revoked: Some(now),
                ..standing
            })
        })
    }

    /// Shares given and received by this account that still stand.
    pub fn shares_of(&self, me: &str, now: u64) -> Result<Vec<Share>, Fault> {
        self.with(|conn| {
            let mut listing = conn.prepare(&format!(
                "SELECT {SHARE_COLUMNS} FROM shares WHERE (owner = ?1 OR grantee = ?1) \
                 AND revoked IS NULL AND (expires IS NULL OR expires > ?2) ORDER BY created"
            ))?;
            let shares = listing
                .query_map(params![me, now as i64], read_share)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(shares)
        })
    }

    #[cfg(test)]
    pub fn share(&self, id: &str) -> Result<Option<Share>, Fault> {
        self.with(|conn| share_by_id(conn, id))
    }

    /// Shares that stand on that device, for whoever has to be told
    /// about it.
    pub fn shares_of_device(&self, device: &str, now: u64) -> Result<Vec<Share>, Fault> {
        self.with(|conn| {
            let mut listing = conn.prepare(&format!(
                "SELECT {SHARE_COLUMNS} FROM shares WHERE device = ?1 AND revoked IS NULL \
                 AND (expires IS NULL OR expires > ?2)"
            ))?;
            let shares = listing
                .query_map(params![device, now as i64], read_share)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(shares)
        })
    }

    /// Under what right the first device may go towards the second.
    pub fn right_to(&self, from: &Device, to: &str, now: u64) -> Result<(Device, Grant), Fault> {
        self.with(|conn| {
            let target = device_by_id(conn, to)?
                .filter(|d| d.revoked.is_none())
                .ok_or(Code::NotFound)?;
            if target.account == from.account {
                return Ok((target, Grant::Owner));
            }
            let share: Option<String> = conn
                .query_row(
                    "SELECT id FROM shares WHERE device = ?1 AND grantee = ?2 AND revoked IS NULL \
                     AND (expires IS NULL OR expires > ?3)",
                    params![target.id, from.account, now as i64],
                    |row| row.get(0),
                )
                .optional()?;
            match share {
                Some(id) => Ok((target, Grant::Share { id })),
                None => Err(Code::NoRight.into()),
            }
        })
    }

    // ---- Sessions --------------------------------------------------

    pub fn session_started(
        &self,
        id: &str,
        from: &str,
        to: &str,
        grant: &Grant,
        now: u64,
    ) -> Result<(), Fault> {
        let grant = serde_json::to_string(grant).map_err(|e| Fault::Broken(e.to_string()))?;
        self.with(|conn| {
            conn.execute(
                "INSERT INTO sessions (id, from_device, to_device, grant, started) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, from, to, grant, now as i64],
            )?;
            conn.execute(
                "DELETE FROM sessions WHERE started < ?1",
                params![now.saturating_sub(SESSIONS_KEPT) as i64],
            )?;
            Ok(())
        })
    }

    pub fn session_ended(&self, id: &str, now: u64) -> Result<(), Fault> {
        self.with(|conn| {
            conn.execute(
                "UPDATE sessions SET ended = ?1 WHERE id = ?2 AND ended IS NULL",
                params![now as i64, id],
            )?;
            Ok(())
        })
    }

    /// Writes down what the relay carried for that session.
    ///
    /// Written by the relay alone, and added to rather than replaced: a
    /// device that came back to the relay after a break has two turns to
    /// its name. A session with nothing written here went direct, which
    /// is what the milestone's own criterion reads.
    pub fn session_relayed(&self, id: &str, bytes: u64) -> Result<(), Fault> {
        self.with(|conn| {
            conn.execute(
                "UPDATE sessions SET road = 'relais', relayed_bytes = relayed_bytes + ?1 \
                 WHERE id = ?2",
                params![bytes as i64, id],
            )?;
            Ok(())
        })
    }

    pub fn counts(&self) -> Result<Counts, Fault> {
        self.with(|conn| {
            let count = |sql: &str| conn.query_row(sql, [], |row| row.get::<_, i64>(0));
            Ok(Counts {
                accounts: count("SELECT COUNT(*) FROM accounts")? as u64,
                devices: count("SELECT COUNT(*) FROM devices WHERE revoked IS NULL")? as u64,
                contacts: count("SELECT COUNT(*) FROM contacts WHERE status = 'accepted'")? as u64,
                shares: count("SELECT COUNT(*) FROM shares WHERE revoked IS NULL")? as u64,
            })
        })
    }

    /// What the relay carried over the sessions still written down.
    pub fn relayed(&self) -> Result<Relayed, Fault> {
        self.with(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(relayed_bytes), 0) FROM sessions \
                 WHERE road = 'relais'",
                [],
                |row| {
                    Ok(Relayed {
                        sessions: row.get::<_, i64>(0)? as u64,
                        bytes: row.get::<_, i64>(1)? as u64,
                    })
                },
            )?)
        })
    }

    /// A coherent copy of the whole file, for the backup.
    pub fn copy_to(&self, path: &Path) -> Result<(), Fault> {
        self.with(|conn| {
            conn.execute("VACUUM INTO ?1", params![path.to_string_lossy()])?;
            Ok(())
        })
    }
}

/// Every shape the schema has had, in order; the file remembers which
/// it is at in its `user_version`.
const MIGRATIONS: &[&str] = &[SCHEMA];

/// Brings the file up to the latest shape, one step at a time, each
/// step whole or not at all.
fn migrate(conn: &mut Connection) -> Result<(), Fault> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let done = usize::try_from(version).unwrap_or(0);
    for (step, sql) in MIGRATIONS.iter().enumerate().skip(done) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", step as i64 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

fn rustls_certificate(der: &[u8]) -> rustls::pki_types::CertificateDer<'_> {
    rustls::pki_types::CertificateDer::from(der)
}

fn issue_token(
    conn: &Connection,
    kind: &str,
    account: &str,
    device: Option<&str>,
    now: u64,
    life: u64,
) -> Result<Token, Fault> {
    let raw = raw_token();
    let expires = now + life;
    conn.execute(
        "INSERT INTO tokens (hash, kind, account, device, created, expires) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            hashed(&raw),
            kind,
            account,
            device,
            now as i64,
            expires as i64
        ],
    )?;
    Ok(Token { raw, expires })
}

fn device_by_id(conn: &Connection, id: &str) -> Result<Option<Device>, Fault> {
    Ok(conn
        .query_row(
            &format!("SELECT {DEVICE_COLUMNS} FROM devices WHERE id = ?1"),
            params![id],
            read_device,
        )
        .optional()?)
}

fn contact_by_id(conn: &Connection, id: &str) -> Result<Option<Contact>, Fault> {
    Ok(conn
        .query_row(
            &format!("SELECT {CONTACT_COLUMNS} FROM contacts WHERE id = ?1"),
            params![id],
            read_contact,
        )
        .optional()?)
}

fn share_by_id(conn: &Connection, id: &str) -> Result<Option<Share>, Fault> {
    Ok(conn
        .query_row(
            &format!("SELECT {SHARE_COLUMNS} FROM shares WHERE id = ?1"),
            params![id],
            read_share,
        )
        .optional()?)
}

/// What revoking a device comes down to in the tables.
fn revoke_device_rows(conn: &Connection, device: &str, now: u64) -> Result<(), Fault> {
    conn.execute(
        "UPDATE devices SET revoked = ?1 WHERE id = ?2",
        params![now as i64, device],
    )?;
    conn.execute("DELETE FROM tokens WHERE device = ?1", params![device])?;
    conn.execute(
        "UPDATE shares SET revoked = ?1 WHERE device = ?2 AND revoked IS NULL",
        params![now as i64, device],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zyr_transport::Identity;

    const PASSWORD: &str = "douze caractères";

    fn store() -> Store {
        Store::in_memory().unwrap()
    }

    fn account(store: &Store, name: &str) -> Account {
        store
            .create_account(name, PASSWORD, None, None, Registration::Open, 1_000)
            .unwrap()
    }

    fn device(store: &Store, account: &Account, name: &str) -> (Device, Token, Identity) {
        let identity = Identity::generate().unwrap();
        let (device, token) = store
            .link_device(&account.id, identity.certificate().as_ref(), name, 1_000)
            .unwrap();
        (device, token, identity)
    }

    fn friends(store: &Store, a: &Account, b: &Account) -> Contact {
        let asked = store.ask_contact(&a.id, &b.username, 1_000).unwrap();
        store.answer_contact(&b.id, &asked.id, true, 1_001).unwrap()
    }

    #[test]
    fn an_account_is_created_once_and_logs_in_with_its_password() {
        let store = store();
        let victor = account(&store, "victor");
        assert_eq!(victor.username, "victor");
        assert!(matches!(
            store
                .create_account("Victor", PASSWORD, None, None, Registration::Open, 1_000)
                .unwrap_err(),
            Fault::Refused(Code::UsernameTaken)
        ));
        let (again, token) = store.login("victor", PASSWORD, 2_000).unwrap();
        assert_eq!(again, victor);
        assert_eq!(token.expires, 2_000 + ACCOUNT_TOKEN_LIFE);
        assert_eq!(store.account_of_token(&token.raw, 2_500).unwrap(), victor);
        // Expiré, ou inconnu : refusé de la même façon.
        assert!(matches!(
            store
                .account_of_token(&token.raw, 2_000 + ACCOUNT_TOKEN_LIFE)
                .unwrap_err(),
            Fault::Refused(Code::Unauthorized)
        ));
        assert!(matches!(
            store.account_of_token("n'importe quoi", 2_500).unwrap_err(),
            Fault::Refused(Code::Unauthorized)
        ));
    }

    #[test]
    fn a_wrong_password_and_an_unknown_name_are_refused_alike() {
        let store = store();
        account(&store, "victor");
        for (name, password) in [("victor", "pas le bon mot de passe"), ("inconnu", PASSWORD)] {
            assert!(matches!(
                store.login(name, password, 2_000).unwrap_err(),
                Fault::Refused(Code::InvalidCredentials)
            ));
        }
    }

    #[test]
    fn names_and_passwords_have_a_shape() {
        let store = store();
        for name in [
            "ab",
            "trop long pour un nom d'utilisateur ici",
            "a b",
            "é.è",
        ] {
            assert!(
                matches!(
                    store
                        .create_account(name, PASSWORD, None, None, Registration::Open, 1)
                        .unwrap_err(),
                    Fault::Refused(Code::InvalidUsername)
                ),
                "{name}"
            );
        }
        assert!(matches!(
            store
                .create_account("victor", "court", None, None, Registration::Open, 1)
                .unwrap_err(),
            Fault::Refused(Code::WeakPassword)
        ));
        assert!(acceptable_username("pc-de.victor_2"));
    }

    #[test]
    fn the_registration_policy_is_honoured() {
        let store = store();
        assert!(matches!(
            store
                .create_account("victor", PASSWORD, None, None, Registration::Closed, 1)
                .unwrap_err(),
            Fault::Refused(Code::RegistrationClosed)
        ));
        assert!(matches!(
            store
                .create_account("victor", PASSWORD, None, None, Registration::Invitation, 1)
                .unwrap_err(),
            Fault::Refused(Code::InvitationInvalid)
        ));
        let code = store.new_invitation(1).unwrap();
        assert_eq!(code.len(), 9, "{code}");
        assert!(matches!(
            store
                .create_account(
                    "victor",
                    PASSWORD,
                    None,
                    Some("XXXX-XXXX"),
                    Registration::Invitation,
                    1
                )
                .unwrap_err(),
            Fault::Refused(Code::InvitationInvalid)
        ));
        store
            .create_account(
                "victor",
                PASSWORD,
                None,
                Some(&code),
                Registration::Invitation,
                2,
            )
            .unwrap();
        // Un code ne sert qu'une fois.
        assert!(matches!(
            store
                .create_account(
                    "autre",
                    PASSWORD,
                    None,
                    Some(&code),
                    Registration::Invitation,
                    3
                )
                .unwrap_err(),
            Fault::Refused(Code::InvitationInvalid)
        ));
        assert_eq!(store.invitations().unwrap()[0].used, Some(2));
        let unused = store.new_invitation(4).unwrap();
        store.revoke_invitation(&unused).unwrap();
        assert!(store.revoke_invitation(&unused).is_err());
    }

    #[test]
    fn a_device_is_attached_and_presents_its_token() {
        let store = store();
        let victor = account(&store, "victor");
        let (device, token, identity) = device(&store, &victor, "PC de Victor");
        assert_eq!(device.fingerprint, identity.fingerprint());
        let bearer = store.bearer_of_token(&token.raw, 1_500).unwrap();
        assert_eq!(bearer.device, device);
        assert_eq!(bearer.account, victor);
        assert!(!bearer.renew);
        // Près de la fin, le jeton demande son renouvellement.
        let late = 1_000 + DEVICE_TOKEN_LIFE - DEVICE_TOKEN_RENEWAL + 1;
        assert!(store.bearer_of_token(&token.raw, late).unwrap().renew);
        let renewed = store.renew_device_token(&device, late).unwrap();
        assert_ne!(renewed.raw, token.raw);
        assert!(store.bearer_of_token(&renewed.raw, late).is_ok());
        assert_eq!(store.devices_of(&victor.id).unwrap(), vec![device]);
    }

    #[test]
    fn attaching_the_same_device_again_keeps_it_and_changes_its_token() {
        let store = store();
        let victor = account(&store, "victor");
        let (first, old_token, identity) = device(&store, &victor, "PC");
        let (again, new_token) = store
            .link_device(
                &victor.id,
                identity.certificate().as_ref(),
                "PC renommé",
                2_000,
            )
            .unwrap();
        assert_eq!(again.id, first.id);
        assert_eq!(again.name, "PC renommé");
        assert!(store.bearer_of_token(&old_token.raw, 2_001).is_err());
        assert!(store.bearer_of_token(&new_token.raw, 2_001).is_ok());
        assert_eq!(store.devices_of(&victor.id).unwrap().len(), 1);
    }

    #[test]
    fn a_device_attached_to_another_account_moves_there() {
        // Un appareil est à son propriétaire : rattaché ailleurs, il quitte
        // le premier compte, dont les partages sur lui tombent.
        let store = store();
        let victor = account(&store, "victor");
        let ami = account(&store, "ami");
        let (first, token, identity) = device(&store, &victor, "PC");
        friends(&store, &victor, &ami);
        let share = store
            .give_share(&victor.id, &first.id, "ami", &Permission::ALL, None, 1_100)
            .unwrap();

        let (moved, _) = store
            .link_device(&ami.id, identity.certificate().as_ref(), "PC", 2_000)
            .unwrap();
        assert_ne!(moved.id, first.id);
        assert_eq!(moved.account, ami.id);
        assert!(store.devices_of(&victor.id).unwrap().is_empty());
        assert!(store.bearer_of_token(&token.raw, 2_001).is_err());
        assert_eq!(
            store.share(&share.id).unwrap().unwrap().revoked,
            Some(2_000)
        );
    }

    #[test]
    fn a_revoked_device_is_gone_with_its_tokens_and_shares() {
        let store = store();
        let victor = account(&store, "victor");
        let ami = account(&store, "ami");
        let (device, token, _) = device(&store, &victor, "PC");
        friends(&store, &victor, &ami);
        store
            .give_share(&victor.id, &device.id, "ami", &Permission::ALL, None, 1_100)
            .unwrap();
        // Seul son compte peut le révoquer.
        assert!(matches!(
            store.revoke_device(&ami.id, &device.id, 2_000).unwrap_err(),
            Fault::Refused(Code::DeviceUnknown)
        ));
        let revoked = store.revoke_device(&victor.id, &device.id, 2_000).unwrap();
        assert_eq!(revoked.revoked, Some(2_000));
        assert!(matches!(
            store.bearer_of_token(&token.raw, 2_001).unwrap_err(),
            Fault::Refused(Code::Unauthorized)
        ));
        assert!(store.devices_of(&victor.id).unwrap().is_empty());
        assert!(store.shares_of(&ami.id, 2_001).unwrap().is_empty());
        assert!(store.revoke_device(&victor.id, &device.id, 2_002).is_err());
    }

    #[test]
    fn a_contact_is_asked_answered_and_removed() {
        let store = store();
        let victor = account(&store, "victor");
        let ami = account(&store, "ami");
        assert!(matches!(
            store.ask_contact(&victor.id, "victor", 1).unwrap_err(),
            Fault::Refused(Code::ContactSelf)
        ));
        assert!(matches!(
            store.ask_contact(&victor.id, "personne", 1).unwrap_err(),
            Fault::Refused(Code::NotFound)
        ));
        let asked = store.ask_contact(&victor.id, "ami", 1_000).unwrap();
        assert!(!asked.accepted);
        // Ni l'un ni l'autre ne redemande tant que ça attend.
        for who in [&victor, &ami] {
            let other = if who.id == victor.id { "ami" } else { "victor" };
            assert!(matches!(
                store.ask_contact(&who.id, other, 1_001).unwrap_err(),
                Fault::Refused(Code::ContactExists)
            ));
        }
        // Seul celui qui a été demandé répond.
        assert!(
            store
                .answer_contact(&victor.id, &asked.id, true, 1_002)
                .is_err()
        );
        let accepted = store
            .answer_contact(&ami.id, &asked.id, true, 1_002)
            .unwrap();
        assert!(accepted.accepted);
        assert_eq!(accepted.other_than(&victor.id), ami.id);
        assert_eq!(store.contacts_of(&ami.id).unwrap(), vec![accepted.clone()]);

        let removed = store
            .remove_contact(&victor.id, &accepted.id, 1_003)
            .unwrap();
        assert_eq!(removed.id, accepted.id);
        assert!(store.contacts_of(&victor.id).unwrap().is_empty());
        // Et on peut redemander ensuite.
        let again = store.ask_contact(&ami.id, "victor", 1_004).unwrap();
        assert!(!again.accepted);
        let declined = store
            .answer_contact(&victor.id, &again.id, false, 1_005)
            .unwrap();
        assert!(!declined.accepted);
        assert!(store.contacts_of(&victor.id).unwrap().is_empty());
    }

    #[test]
    fn a_share_names_one_machine_and_one_contact_and_gives_a_right() {
        let store = store();
        let victor = account(&store, "victor");
        let ami = account(&store, "ami");
        let etranger = account(&store, "etranger");
        let (pc, _, _) = device(&store, &victor, "PC de Victor");
        let (portable, _, _) = device(&store, &ami, "Portable");
        let (autre, _, _) = device(&store, &etranger, "Autre");

        // Pas de partage sans contact accepté.
        assert!(matches!(
            store
                .give_share(&victor.id, &pc.id, "ami", &Permission::ALL, None, 1_100)
                .unwrap_err(),
            Fault::Refused(Code::NotAContact)
        ));
        friends(&store, &victor, &ami);
        // Ni sur une machine qui n'est pas la sienne.
        assert!(matches!(
            store
                .give_share(
                    &victor.id,
                    &portable.id,
                    "ami",
                    &Permission::ALL,
                    None,
                    1_100
                )
                .unwrap_err(),
            Fault::Refused(Code::ShareInvalid)
        ));
        let share = store
            .give_share(
                &victor.id,
                &pc.id,
                "ami",
                &Permission::ALL,
                Some(5_000),
                1_100,
            )
            .unwrap();
        assert_eq!(share.grantee, ami.id);

        // Le droit : le sien, le partagé, rien.
        let (_, right) = store.right_to(&portable, &pc.id, 1_200).unwrap();
        assert_eq!(
            right,
            Grant::Share {
                id: share.id.clone()
            }
        );
        let (_, own) = store.right_to(&pc, &pc.id, 1_200).unwrap();
        assert_eq!(own, Grant::Owner);
        assert!(matches!(
            store.right_to(&autre, &pc.id, 1_200).unwrap_err(),
            Fault::Refused(Code::NoRight)
        ));
        // Expiré, plus de droit ; retiré, plus de droit.
        assert!(store.right_to(&portable, &pc.id, 5_000).is_err());
        assert!(store.shares_of(&ami.id, 5_000).unwrap().is_empty());
        assert_eq!(store.shares_of(&ami.id, 1_200).unwrap().len(), 1);

        // Redonné, c'est le même partage, changé.
        let again = store
            .give_share(
                &victor.id,
                &pc.id,
                "ami",
                &[Permission::Connect],
                None,
                1_300,
            )
            .unwrap();
        assert_eq!(again.id, share.id);
        assert_eq!(again.permissions, [Permission::Connect]);
        assert_eq!(again.expires, None);

        let removed = store.remove_share(&ami.id, &share.id, 1_400).unwrap();
        assert_eq!(removed.revoked, Some(1_400));
        assert!(store.right_to(&portable, &pc.id, 1_401).is_err());
        assert!(store.remove_share(&ami.id, &share.id, 1_402).is_err());
    }

    #[test]
    fn ending_a_contact_takes_the_shares_between_the_two_with_it() {
        let store = store();
        let victor = account(&store, "victor");
        let ami = account(&store, "ami");
        let (pc, _, _) = device(&store, &victor, "PC");
        let contact = friends(&store, &victor, &ami);
        let share = store
            .give_share(&victor.id, &pc.id, "ami", &Permission::ALL, None, 1_100)
            .unwrap();
        store.remove_contact(&ami.id, &contact.id, 1_200).unwrap();
        assert_eq!(
            store.share(&share.id).unwrap().unwrap().revoked,
            Some(1_200)
        );
    }

    #[test]
    fn the_journal_of_sessions_says_who_met_whom_and_forgets_old_ones() {
        let store = store();
        store
            .session_started("s1", "d1", "d2", &Grant::Owner, 1_000)
            .unwrap();
        store.session_ended("s1", 1_500).unwrap();
        let far_later = 1_000 + SESSIONS_KEPT + 1;
        store
            .session_started("s2", "d1", "d2", &Grant::Owner, far_later)
            .unwrap();
        let kept: i64 = store
            .with(|conn| Ok(conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(kept, 1);
    }

    #[test]
    fn what_the_relay_carried_is_counted_and_a_direct_session_counts_nothing() {
        // Le critère du jalon se lit ici : en direct, le compteur du
        // serveur reste à zéro, et il n'y a même pas de ligne relayée.
        let store = store();
        for session in ["direct", "relayee"] {
            store
                .session_started(session, "d1", "d2", &Grant::Owner, 1_000)
                .unwrap();
        }
        store.session_relayed("relayee", 1_200).unwrap();
        // Un appareil revenu au relais après une coupure a deux tours à
        // son nom : ce qu'il a porté s'ajoute.
        store.session_relayed("relayee", 800).unwrap();
        for session in ["direct", "relayee"] {
            store.session_ended(session, 1_500).unwrap();
        }
        assert_eq!(
            store.relayed().unwrap(),
            Relayed {
                sessions: 1,
                bytes: 2_000
            }
        );
    }

    #[test]
    fn the_schema_is_brought_up_once_and_remembered() {
        // Rouvrir le même fichier ne rejoue rien : la version est écrite
        // dedans, et une étape déjà faite ne se refait pas.
        let store = store();
        let version = store
            .with(|conn| {
                Ok(conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?)
            })
            .unwrap();
        assert_eq!(version as usize, MIGRATIONS.len());
        store
            .with(|conn| {
                let mut conn_again = Connection::open_in_memory()?;
                conn_again.execute_batch(SCHEMA)?;
                conn_again.pragma_update(None, "user_version", 1)?;
                migrate(&mut conn_again)?;
                let _ = conn;
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn an_account_can_be_reset_and_deleted_by_the_administrator() {
        let store = store();
        let victor = account(&store, "victor");
        let (_, token) = store.login("victor", PASSWORD, 1_000).unwrap();
        store
            .reset_password("victor", "un autre mot de passe")
            .unwrap();
        assert!(store.account_of_token(&token.raw, 1_001).is_err());
        assert!(store.login("victor", PASSWORD, 1_002).is_err());
        assert!(
            store
                .login("victor", "un autre mot de passe", 1_002)
                .is_ok()
        );
        device(&store, &victor, "PC");
        store.delete_account("victor").unwrap();
        assert!(store.account_by_username("victor").unwrap().is_none());
        assert_eq!(store.counts().unwrap(), Counts::default());
        assert!(store.delete_account("victor").is_err());
    }
}
