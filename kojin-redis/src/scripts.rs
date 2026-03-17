/// Lua script: atomically move a message from the processing list and store the result.
/// Used for ack operations.
pub const ACK_SCRIPT: &str = r#"
local processing_key = KEYS[1]
local msg_id = ARGV[1]
redis.call('LREM', processing_key, 1, msg_id)
return 1
"#;

/// Lua script: move due items from the scheduled sorted set to their target queues.
/// Score represents the unix timestamp; items with score <= now are due.
pub const POLL_SCHEDULED_SCRIPT: &str = r#"
local scheduled_key = KEYS[1]
local now = tonumber(ARGV[1])
local items = redis.call('ZRANGEBYSCORE', scheduled_key, '-inf', now, 'LIMIT', 0, 100)
if #items == 0 then
    return 0
end
for _, item in ipairs(items) do
    local msg = cjson.decode(item)
    local queue_key = ARGV[2] .. ':queue:' .. msg.queue
    redis.call('LPUSH', queue_key, item)
end
redis.call('ZREMRANGEBYSCORE', scheduled_key, '-inf', now)
return #items
"#;
