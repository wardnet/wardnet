//! DDNS unit tests. Provider/HTTP behaviour is exercised against `wiremock`
//! tenants/ddns servers; service-layer logic uses hand-written mock
//! repositories.

mod cloudflare;
mod public_ip;
mod region;
mod runner;
mod service;
mod settings;
