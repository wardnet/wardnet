//! DDNS unit tests. Provider/HTTP behaviour is exercised against `wiremock`
//! servers (the only real contract check — bridge end-to-end is deferred);
//! service-layer logic uses hand-written mock repositories.

mod bridge;
mod cloudflare;
mod public_ip;
mod region;
mod runner;
mod service;
