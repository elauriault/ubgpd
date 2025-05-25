// src/neighbor/timers.rs

use super::session::BGPNeighbor;
use super::types::Event;
use anyhow::{anyhow, Context, Result};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

use crate::neighbor::BGPState;

pub async fn timer_hold(
    n: Arc<Mutex<BGPNeighbor>>,
    mut receiver: tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    loop {
        let (hold_time, tx) = {
            let n = n.lock().await;
            (n.attributes.hold_time, n.tx.clone())
        };

        let tx = tx.ok_or_else(|| anyhow!("Timer channel not available for neighbor"))?;

        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(hold_time as u64 / 3)) => {
                tx.send(Event::KeepaliveTimerExpires)
                    .await
                    .context("Failed to send KeepaliveTimerExpires event")?;
            }
            _ = &mut receiver => {
                log::debug!("Exiting hold timer");
                return Ok(());
            }
        }
    }
}

pub async fn timer_keepalive(n: Arc<Mutex<BGPNeighbor>>, tx: mpsc::Sender<Event>) -> Result<()> {
    log::debug!("FSM Starting TimerKeepalive");

    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;

        let (keepalive_timer, state, hold_time) = {
            let mut n = n.lock().await;
            n.attributes.keepalive_timer += 1;
            (
                n.attributes.keepalive_timer,
                n.attributes.state,
                n.attributes.hold_time as usize,
            )
        };

        if state == BGPState::Idle {
            log::info!("FSM TimerKeepalive exiting due to Idle state");
            return Ok(());
        }

        log::trace!("FSM TimerKeepalive incremented to {}", keepalive_timer);

        if keepalive_timer > hold_time {
            tx.send(Event::KeepaliveTimerExpires)
                .await
                .context("Failed to send KeepaliveTimerExpires event")?;
        }
    }
}
