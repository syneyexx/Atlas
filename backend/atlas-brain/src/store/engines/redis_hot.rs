use crate::error::{AtlasError, AtlasResult};
use crate::memory::working::WorkingMemoryState;
use redis::Commands;
use std::sync::Mutex;

pub struct RedisHot {
    conn: Mutex<redis::Connection>,
}

impl RedisHot {
    pub fn connect(url: &str) -> AtlasResult<Self> {
        let client = redis::Client::open(url).map_err(|e| {
            AtlasError::Upstream(format!("redis client: {e}"))
        })?;
        let conn = client
            .get_connection()
            .map_err(|e| AtlasError::Upstream(format!("redis connect: {e}")))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn working_key(conversation_id: &str) -> String {
        format!("atlas:working:{conversation_id}")
    }

    fn cache_key(key: &str) -> String {
        format!("atlas:cache:{key}")
    }

    pub fn get_working(&self, conversation_id: &str) -> AtlasResult<Option<WorkingMemoryState>> {
        let mut conn = self.conn.lock().expect("redis lock");
        let raw: Option<String> = conn
            .get(Self::working_key(conversation_id))
            .map_err(|e| AtlasError::Upstream(format!("redis get working: {e}")))?;
        match raw {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    pub fn set_working(
        &self,
        state: &WorkingMemoryState,
        ttl_secs: u64,
    ) -> AtlasResult<()> {
        let mut conn = self.conn.lock().expect("redis lock");
        let payload = serde_json::to_string(state)?;
        conn.set_ex::<_, _, ()>(Self::working_key(&state.conversation_id), payload, ttl_secs)
            .map_err(|e| AtlasError::Upstream(format!("redis set working: {e}")))?;
        Ok(())
    }

    pub fn delete_working(&self, conversation_id: &str) -> AtlasResult<bool> {
        let mut conn = self.conn.lock().expect("redis lock");
        let n: i32 = conn
            .del(Self::working_key(conversation_id))
            .map_err(|e| AtlasError::Upstream(format!("redis del working: {e}")))?;
        Ok(n > 0)
    }

    pub fn cache_get(&self, key: &str) -> AtlasResult<Option<(String, String)>> {
        let mut conn = self.conn.lock().expect("redis lock");
        let raw: Option<String> = conn
            .get(Self::cache_key(key))
            .map_err(|e| AtlasError::Upstream(format!("redis cache get: {e}")))?;
        match raw {
            Some(s) => {
                let v: serde_json::Value = serde_json::from_str(&s)?;
                Ok(Some((
                    v.get("response")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    v.get("model_id")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                )))
            }
            None => Ok(None),
        }
    }

    pub fn cache_put(
        &self,
        key: &str,
        response: &str,
        model_id: &str,
        ttl_secs: u64,
    ) -> AtlasResult<()> {
        let mut conn = self.conn.lock().expect("redis lock");
        let payload = serde_json::json!({
            "response": response,
            "model_id": model_id,
        })
        .to_string();
        conn.set_ex::<_, _, ()>(Self::cache_key(key), payload, ttl_secs.max(60))
            .map_err(|e| AtlasError::Upstream(format!("redis cache put: {e}")))?;
        Ok(())
    }

    pub fn cache_clear(&self) -> AtlasResult<usize> {
        let mut conn = self.conn.lock().expect("redis lock");
        let keys: Vec<String> = conn
            .keys("atlas:cache:*")
            .map_err(|e| AtlasError::Upstream(format!("redis keys: {e}")))?;
        if keys.is_empty() {
            return Ok(0);
        }
        let n: i32 = conn
            .del(keys.as_slice())
            .map_err(|e| AtlasError::Upstream(format!("redis del cache: {e}")))?;
        Ok(n as usize)
    }
}
