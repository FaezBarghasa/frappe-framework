use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use actix_web::web::{Bytes, Data, ReqData, Query};
use actix_web::{HttpResponse, Responder};
use futures_util::Stream;
use serde::{Serialize, Deserialize};
use crate::middleware::tenant::TenantContext;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SseEvent {
    pub tenant_id: String,
    pub topic: String,
    pub data: serde_json::Value,
}

pub struct EventBroadcaster {
    sender: broadcast::Sender<SseEvent>,
}

impl EventBroadcaster {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1024);
        Self { sender }
    }

    pub fn publish(&self, event: SseEvent) {
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SseEvent> {
        self.sender.subscribe()
    }
}

pub struct SseStream {
    rx: mpsc::Receiver<Result<Bytes, std::convert::Infallible>>,
}

impl Stream for SseStream {
    type Item = Result<Bytes, actix_web::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(Ok(bytes))) => Poll::Ready(Some(Ok(bytes))),
            Poll::Ready(Some(Err(inf))) => match inf {},
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[derive(Deserialize)]
pub struct SseQuery {
    pub topic: Option<String>,
}

pub async fn sse_handler(
    broadcaster: Data<EventBroadcaster>,
    tenant: ReqData<TenantContext>,
    query: Query<SseQuery>,
) -> impl Responder {
    let tenant_id = tenant.tenant_id.clone();
    let topic = query.topic.clone();
    let mut rx = broadcaster.subscribe();
    
    let (tx, rx_stream) = mpsc::channel::<Result<Bytes, std::convert::Infallible>>(100);
    
    tokio::spawn(async move {
        // Send initial connection message and heartbeat/retry settings
        if tx.send(Ok(Bytes::from("retry: 10000\n\n"))).await.is_err() {
            return;
        }
        
        while let Ok(event) = rx.recv().await {
            if event.tenant_id == tenant_id {
                let matches_topic = match &topic {
                    Some(t) => event.topic == *t,
                    None => true,
                };
                
                if matches_topic {
                    let event_str = match serde_json::to_string(&event.data) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    
                    let sse_msg = format!("event: {}\ndata: {}\n\n", event.topic, event_str);
                    if tx.send(Ok(Bytes::from(sse_msg))).await.is_err() {
                        break;
                    }
                }
            }
        }
    });
    
    HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .streaming(SseStream { rx: rx_stream })
}
