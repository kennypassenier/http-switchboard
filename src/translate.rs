//! The pure translation core (L2: K5, K12, K14; AR1, AR7).
//!
//! Pure by contract: no network, no clock, no environment. AR7 will run
//! minijinja here with JSON autoescape on and strict undefined, so a
//! quote in an alert summary cannot rewrite the document and a renamed
//! field errors instead of rendering empty.
