// src/neighbor/timers.rs

use super::session::BGPNeighbor;
use super::types::Event;
use async_std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

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
            println!("Exiting hold timer");
            break;
        }
        tx.send(Event::KeepaliveTimerExpires).await.unwrap();
    }
}

pub async fn timer_keepalive(n: Arc<Mutex<BGPNeighbor>>, tx: mpsc::Sender<Event>) {
    println!("FSM: Starting TimerKeepalive");
    loop {
        sleep(Duration::from_secs(1)).await;
        let k;
        let h;
        {
            let mut n = n.lock().await;
            n.attributes.keepalive_timer += 1;
            k = n.attributes.keepalive_timer;
            h = n.attributes.hold_time as usize;
        }
        println!("FSM: TimerKeepalive incremented");
        if k > h {
            tx.send(Event::KeepaliveTimerExpires).await.unwrap()
        }
    }
}
