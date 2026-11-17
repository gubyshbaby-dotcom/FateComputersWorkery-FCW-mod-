//! Одноразовые токены привязки клиента к машине (IssueToken / Attach).

use base64::Engine;
use rand::RngCore;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct TokenInfo {
    pub vm_id: String,
    pub player_uuid: String,
    pub player_name: String,
    pub permissions: u32,
    pub expires: Instant,
}

#[derive(Default)]
pub struct TokenStore {
    map: HashMap<String, TokenInfo>,
}

pub const DEFAULT_TTL_SECS: u32 = 120;

impl TokenStore {
    /// Выпускает токен: 32 случайных байта в base64url без выравнивания.
    /// Возвращает токен и срок действия (Unix, мс).
    pub fn issue(
        &mut self,
        vm_id: &str,
        player_uuid: &str,
        player_name: &str,
        permissions: u32,
        ttl_secs: u32,
    ) -> (String, u64) {
        self.gc();
        let ttl = if ttl_secs == 0 { DEFAULT_TTL_SECS } else { ttl_secs };
        let mut raw = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut raw);
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);
        self.map.insert(
            token.clone(),
            TokenInfo {
                vm_id: vm_id.to_string(),
                player_uuid: player_uuid.to_string(),
                player_name: player_name.to_string(),
                permissions,
                expires: Instant::now() + Duration::from_secs(ttl as u64),
            },
        );
        (token, crate::unix_ms() + ttl as u64 * 1000)
    }

    /// Предъявление токена: токен расходуется.
    pub fn take(&mut self, token: &str) -> Option<TokenInfo> {
        self.gc();
        self.map.remove(token)
    }

    /// Отзывает неиспользованные токены. Пустые поля — любые.
    pub fn revoke(&mut self, vm_id: &str, player_uuid: &str) -> usize {
        let before = self.map.len();
        self.map.retain(|_, t| {
            !((vm_id.is_empty() || t.vm_id == vm_id) && (player_uuid.is_empty() || t.player_uuid == player_uuid))
        });
        before - self.map.len()
    }

    fn gc(&mut self) {
        let now = Instant::now();
        self.map.retain(|_, t| t.expires > now);
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_take_once() {
        let mut s = TokenStore::default();
        let (t, exp) = s.issue("vm1", "p1", "Steve", 7, 0);
        assert_eq!(t.len(), 43);
        assert!(!t.contains('+') && !t.contains('/') && !t.contains('='));
        assert!(exp > crate::unix_ms() + 100_000);
        let info = s.take(&t).unwrap();
        assert_eq!(info.vm_id, "vm1");
        assert_eq!(info.permissions, 7);
        assert!(s.take(&t).is_none(), "token is single-use");
    }

    #[test]
    fn revoke_filters() {
        let mut s = TokenStore::default();
        s.issue("vm1", "p1", "a", 1, 60);
        s.issue("vm1", "p2", "b", 1, 60);
        s.issue("vm2", "p1", "a", 1, 60);
        assert_eq!(s.revoke("vm1", "p1"), 1);
        assert_eq!(s.revoke("", "p1"), 1);
        assert_eq!(s.revoke("", ""), 1);
        assert!(s.is_empty());
    }
}
