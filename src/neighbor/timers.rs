// src/neighbor/timers.rs

use super::session::BGPNeighbor;
use super::types::Event;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

use crate::neighbor::BGPState;

pub async fn timer_hold(
    n: Arc<Mutex<BGPNeighbor>>,
    mut receiver: tokio::sync::oneshot::Receiver<()>,
) {
    loop {
        let s;
        let tx;
        {
            let n = n.lock().await;
            s = n.attributes.hold_time;
            tx = n.tx.clone();
        }
        let tx = tx.unwrap();
        sleep(Duration::from_secs(s as u64 / 3)).await;
        if receiver.try_recv().is_ok() {
            log::debug!("Exiting hold timer");
            break;
        }
        tx.send(Event::KeepaliveTimerExpires).await.unwrap();
    }
}

pub async fn timer_keepalive(n: Arc<Mutex<BGPNeighbor>>, tx: mpsc::Sender<Event>) {
    log::debug!("FSM Starting TimerKeepalive");
    loop {
        sleep(Duration::from_secs(1)).await;
        let k;
        let h;
        let s;
        {
            let mut n = n.lock().await;
            n.attributes.keepalive_timer += 1;
            k = n.attributes.keepalive_timer;
            s = n.attributes.state;
            h = n.attributes.hold_time as usize;
        }
        if s == BGPState::Idle {
            log::info!("FSM TimerKeepalive exiting due to Idle state");
            break;
        }
        log::debug!("FSM TimerKeepalive incremented");
        if k > h {
            tx.send(Event::KeepaliveTimerExpires).await.unwrap()
        }
    }
    log::info!("TimerKeepalive thread terminated");
}
