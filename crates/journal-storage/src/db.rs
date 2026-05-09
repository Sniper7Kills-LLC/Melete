use std::path::Path;

use rusqlite::Connection;

use crate::error::Result;
use crate::multi_file_backend::init_index_schema;
use crate::schema::init_schema;

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        // Special-case `:memory:` so callers can hold an in-memory DB via Path API.
        let mut conn = if path == Path::new(":memory:") {
            Connection::open_in_memory()?
        } else {
            Connection::open(path)?
        };
        Self::configure(&conn)?;
        init_schema(&conn)?;
        // Single-file backend keeps notebook content + catalog in one
        // file; the multi-file backend splits them. The catalog
        // tables (brushes / templates) are required by the trait
        // surface so callers don't have to know which backend they
        // hold.
        init_index_schema(&mut conn)?;
        Ok(Self { conn })
    }

    pub fn open_in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        Self::configure(&conn)?;
        init_schema(&conn)?;
        init_index_schema(&mut conn)?;
        Ok(Self { conn })
    }

    fn configure(conn: &Connection) -> Result<()> {
        // WAL is set via query_row because PRAGMA journal_mode returns the new mode.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}
