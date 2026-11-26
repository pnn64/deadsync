use std::sync::LazyLock;
use std::time::Duration;

pub(crate) use deadlib_net::{
    AgentConfig, HttpAgent, NetworkError, error_from_ureq, get_json_with, is_timeout_message,
    log_body_snippet, post_json_with, read_bytes_body_bounded, read_json_body, read_text_body_bounded,
    read_text_body_or_empty,
};

// Match Simply Love / ITGmania's GrooveStats request timeout.
pub(crate) const GROOVESTATS_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const STREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const STREAM_RESOLVE_TIMEOUT: Duration = Duration::from_secs(10);

// Score submits and leaderboard reads share one connection pool.
static DEFAULT_AGENT: LazyLock<HttpAgent> = LazyLock::new(|| build_agent(AgentConfig::default()));

// GrooveStats and BoogieStats share a pool with the longer request timeout.
static GROOVESTATS_AGENT: LazyLock<HttpAgent> = LazyLock::new(|| {
    build_agent(AgentConfig {
        timeout: GROOVESTATS_REQUEST_TIMEOUT,
    })
});

// Large downloads share a pool with a deadline only for connection setup.
static STREAMING_AGENT: LazyLock<HttpAgent> = LazyLock::new(|| {
    deadlib_net::build_streaming_agent(
        STREAM_CONNECT_TIMEOUT,
        STREAM_RESOLVE_TIMEOUT,
        deadsync_version::user_agent(),
    )
});

pub(crate) fn build_agent(config: AgentConfig) -> HttpAgent {
    deadlib_net::build_agent(config, deadsync_version::user_agent())
}

pub(crate) fn build_agent_with_connect(config: AgentConfig, connect: Duration) -> HttpAgent {
    deadlib_net::build_agent_with_connect(config, connect, deadsync_version::user_agent())
}

pub(crate) fn build_agent_with_pool(config: AgentConfig, per_host: usize) -> HttpAgent {
    deadlib_net::build_agent_with_pool(config, per_host, deadsync_version::user_agent())
}

pub(crate) fn get_agent() -> HttpAgent {
    DEFAULT_AGENT.clone()
}

pub(crate) fn get_groovestats_agent() -> HttpAgent {
    GROOVESTATS_AGENT.clone()
}

pub(crate) fn get_streaming_agent() -> HttpAgent {
    STREAMING_AGENT.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_agents_keep_their_request_deadlines() {
        assert_eq!(
            get_agent().config().timeouts().global,
            Some(Duration::from_secs(10)),
        );
        assert_eq!(
            get_groovestats_agent().config().timeouts().global,
            Some(Duration::from_secs(60)),
        );
        let streaming = get_streaming_agent();
        let timeouts = streaming.config().timeouts();
        assert_eq!(timeouts.global, None);
        assert_eq!(timeouts.connect, Some(Duration::from_secs(15)));
        assert_eq!(timeouts.resolve, Some(Duration::from_secs(10)));
    }
}
