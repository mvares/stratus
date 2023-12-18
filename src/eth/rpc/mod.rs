//! Ethereum RPC server.

mod rpc_context;
mod rpc_server;

pub use rpc_context::RpcContext;
pub use rpc_server::serve_rpc;