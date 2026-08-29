//! Everything that touches the outside world (L3-L5).
//!
//! Sources (inbound HTTP, kyu topic) and sinks (URL, kyu topic) sit
//! behind traits together with the clock, so the pump can be tested with
//! fakes (AR1) while the E2E suite still runs against a real kyu (S5).
