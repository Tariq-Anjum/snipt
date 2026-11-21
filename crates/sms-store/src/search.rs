//! Search with the semantics of store.ts `filterThreads`: every whitespace token
//! (JS-lowercased) must be a substring of the thread haystack = display name,
//! sender, "rcs"/"sms", and for each of the thread's latest 60 messages: body,
//! OTP code, merchant, payment method, reference, reminder reference.
//!
//! FTS5 trigram narrows candidates for tokens of ≥ 3 characters that it can
//! represent faithfully (`fts_usable`); every candidate is then verified
//! exactly in Rust with JS lowercase semantics, so results never include false
//! positives. Other tokens scan the windows.

use std::collections::{HashMap, HashSet};

use rusqlite::params;
use sms_engine::inbox::{InboxFilter, search_tokens};

use crate::{
    Result, Store,
    threads::{THREAD_COLS, ThreadRow, attach_previews, filter_sql, thread_from_row},
};

const WINDOW: i64 = 60;

/// FTS5 phrase literal for a token (double quotes doubled).
fn fts_phrase(tok: &str) -> String {
    format!("\"{}\"", tok.replace('"', "\"\""))
}

/// True when the FTS5 trigram index can narrow candidates for `tok` without
/// false negatives: at least 3 chars, no NUL (the FTS5 query parser cannot
/// represent it), and every char is ASCII or caseless and not a combining
/// mark. SQLite's trigram case folding differs from JS `toLowerCase()` for
/// some cased non-ASCII text (`İ` → `i̇`, final sigma `ς`), so such tokens use
/// the exact scan instead. The only non-ASCII chars whose JS lowercase is
/// ASCII (U+212A KELVIN SIGN → `k`, U+0130 → `i` + U+0307) fold compatibly
/// in SQLite or leave a combining mark in the token.
fn fts_usable(tok: &str) -> bool {
    tok.chars().count() >= 3
        && tok.chars().all(|c| {
            c != '\0'
                && (c.is_ascii()
                    || (!('\u{300}'..='\u{36F}').contains(&c)
                        && c.to_lowercase().eq([c])
                        && c.to_uppercase().eq([c])))
        })
}

impl Store {
    /// Threads matching filter + SIM + query, in list order (latest first).
    pub fn search(
        &self,
        filter: InboxFilter,
        sim: Option<u8>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ThreadRow>> {
        let tokens = search_tokens(query);
        let (pred, args) = filter_sql(filter, sim);
        let sql = format!(
            "SELECT {THREAD_COLS} FROM threads WHERE {pred} ORDER BY latest_ts DESC, thread_id DESC"
        );
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let candidates: Vec<ThreadRow> = stmt
            .query_map(rusqlite::params_from_iter(args), thread_from_row)?
            .map(|r| r.map_err(Into::into).and_then(|(t,)| t.finish()))
            .collect::<Result<_>>()?;
        if tokens.is_empty() {
            let mut rows: Vec<ThreadRow> = candidates.into_iter().take(limit).collect();
            attach_previews(&self.conn, &mut rows)?;
            return Ok(rows);
        }

        // Per-thread recency cutoff (ts, ins_seq) of the 60th newest message.
        let mut cutoff_stmt = self.conn.prepare_cached(
            "SELECT ts_ms, ins_seq FROM messages WHERE thread_id = ?1
             ORDER BY ts_ms DESC, ins_seq DESC LIMIT 1 OFFSET ?2",
        )?;
        let mut cutoffs: HashMap<i64, Option<(i64, i64)>> = HashMap::new();
        for t in &candidates {
            let c = if t.message_count > WINDOW {
                Some(
                    cutoff_stmt.query_row(params![t.thread_id, WINDOW - 1], |r| {
                        Ok((r.get(0)?, r.get(1)?))
                    })?,
                )
            } else {
                None
            };
            cutoffs.insert(t.thread_id, c);
        }
        let in_window = |thread: i64, ts: i64, seq: i64| match cutoffs.get(&thread) {
            Some(Some((cts, cseq))) => (ts, seq) >= (*cts, *cseq),
            Some(None) => true,
            None => false,
        };

        let mut matched: Option<HashSet<i64>> = None;
        for tok in &tokens {
            let mut hits: HashSet<i64> = candidates
                .iter()
                .filter(|t| {
                    t.display_name.to_lowercase().contains(tok.as_str())
                        || t.sender.to_lowercase().contains(tok.as_str())
                        || (if t.is_rcs_capable { "rcs" } else { "sms" }).contains(tok.as_str())
                })
                .map(|t| t.thread_id)
                .collect();

            let rows: Vec<(i64, i64, i64, String, String)> = if fts_usable(tok) {
                let mut s = self.conn.prepare_cached(
                    "SELECT m.thread_id, m.ts_ms, m.ins_seq, m.body, m.search_extra
                     FROM messages_fts f JOIN messages m ON m.id = f.rowid
                     WHERE messages_fts MATCH ?1",
                )?;
                s.query_map([fts_phrase(tok)], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })?
                .collect::<rusqlite::Result<_>>()?
            } else {
                let mut s = self.conn.prepare_cached(
                    "SELECT thread_id, ts_ms, ins_seq, body, search_extra FROM messages",
                )?;
                s.query_map([], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })?
                .collect::<rusqlite::Result<_>>()?
            };
            for (thread, ts, seq, body, extra) in rows {
                if hits.contains(&thread) || !in_window(thread, ts, seq) {
                    continue;
                }
                if body.to_lowercase().contains(tok.as_str())
                    || extra.to_lowercase().contains(tok.as_str())
                {
                    hits.insert(thread);
                }
            }
            matched = Some(match matched {
                None => hits,
                Some(prev) => prev.intersection(&hits).copied().collect(),
            });
            if matched.as_ref().is_some_and(HashSet::is_empty) {
                break;
            }
        }
        let matched = matched.unwrap_or_default();
        let mut rows: Vec<ThreadRow> = candidates
            .into_iter()
            .filter(|t| matched.contains(&t.thread_id))
            .take(limit)
            .collect();
        attach_previews(&self.conn, &mut rows)?;
        Ok(rows)
    }
}
