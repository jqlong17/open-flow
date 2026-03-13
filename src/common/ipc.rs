use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonMessage {
    StartRecording,
    StopRecording,
    GetStatus,
    StopDaemon,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonResponse {
    Ok,
    Status {
        is_recording: bool,
        model_loaded: bool,
        uptime_secs: u64,
    },
    Error(String),
}

pub struct IpcClient;

impl IpcClient {
    pub fn connect() -> Result<Self> {
        // TODO: Implement Unix domain socket connection
        Ok(Self)
    }

    pub fn send(&self, message: DaemonMessage) -> Result<DaemonResponse> {
        // TODO: Serialize and send message
        // TODO: Receive and deserialize response
        Ok(DaemonResponse::Ok)
    }
}

pub struct IpcServer;

impl IpcServer {
    pub fn bind() -> Result<Self> {
        // TODO: Bind Unix domain socket
        Ok(Self)
    }

    pub fn recv(&self) -> Result<(Option<DaemonMessage>, IpcResponder)> {
        // TODO: Receive message
        // TODO: Return message and responder
        Ok((None, IpcResponder))
    }
}

pub struct IpcResponder;

impl IpcResponder {
    pub fn respond(&self, response: DaemonResponse) -> Result<()> {
        // TODO: Send response
        Ok(())
    }
}
