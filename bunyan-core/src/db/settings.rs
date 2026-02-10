use rusqlite::{params, Connection};

use crate::error::{BunyanError, Result};
use crate::models::Setting;

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub fn get(conn: &Connection, key: &str) -> Result<Setting> {
    let mut stmt =
        conn.prepare("SELECT key, value, created_at, updated_at FROM settings WHERE key = ?1")?;
    stmt.query_row([key], |row| {
        Ok(Setting {
            key: row.get(0)?,
            value: row.get(1)?,
            created_at: row.get(2)?,
            updated_at: row.get(3)?,
        })
    })
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            BunyanError::NotFound(format!("Setting not found: {}", key))
        }
        _ => BunyanError::Database(e),
    })
}

pub fn set(conn: &Connection, key: &str, value: &str) -> Result<Setting> {
    let ts = now();
    conn.execute(
        "INSERT INTO settings (key, value, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, value, ts, ts],
    )?;
    get(conn, key)
}

pub fn get_all(conn: &Connection) -> Result<Vec<Setting>> {
    let mut stmt =
        conn.prepare("SELECT key, value, created_at, updated_at FROM settings ORDER BY key ASC")?;
    let settings = stmt
        .query_map([], |row| {
            Ok(Setting {
                key: row.get(0)?,
                value: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(settings)
}

#[allow(dead_code)]
pub fn delete(conn: &Connection, key: &str) -> Result<()> {
    let affected = conn.execute("DELETE FROM settings WHERE key = ?1", [key])?;
    if affected == 0 {
        return Err(BunyanError::NotFound(format!(
            "Setting not found: {}",
            key
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::initialize_database;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        initialize_database(&conn).unwrap();
        conn
    }

    #[test]
    fn set_and_get_returns_matching_value() {
        let conn = test_db();
        let setting = set(&conn, "theme", "dark").unwrap();
        assert_eq!(setting.key, "theme");
        assert_eq!(setting.value, "dark");

        let fetched = get(&conn, "theme").unwrap();
        assert_eq!(fetched.value, "dark");
    }

    #[test]
    fn set_same_key_twice_updates_not_duplicates() {
        let conn = test_db();
        set(&conn, "color", "red").unwrap();
        set(&conn, "color", "blue").unwrap();

        let fetched = get(&conn, "color").unwrap();
        assert_eq!(fetched.value, "blue");

        let all = get_all(&conn).unwrap();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn get_nonexistent_key_returns_not_found() {
        let conn = test_db();
        let result = get(&conn, "nonexistent");
        assert!(matches!(result, Err(BunyanError::NotFound(_))));
    }

    #[test]
    fn get_all_returns_all_set_keys() {
        let conn = test_db();
        set(&conn, "a", "1").unwrap();
        set(&conn, "b", "2").unwrap();
        set(&conn, "c", "3").unwrap();

        let all = get_all(&conn).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].key, "a");
        assert_eq!(all[1].key, "b");
        assert_eq!(all[2].key, "c");
    }

    #[test]
    fn delete_removes_setting() {
        let conn = test_db();
        set(&conn, "temp", "value").unwrap();
        delete(&conn, "temp").unwrap();

        let result = get(&conn, "temp");
        assert!(matches!(result, Err(BunyanError::NotFound(_))));
    }

    #[test]
    fn delete_nonexistent_returns_not_found() {
        let conn = test_db();
        let result = delete(&conn, "nope");
        assert!(matches!(result, Err(BunyanError::NotFound(_))));
    }
}
