use futures::stream::TryStreamExt;
use rtnetlink::packet::RouteMessage;
use rtnetlink::{new_connection, IpVersion};

#[derive(Debug, Default, PartialEq)]
struct Fib {
    routes: Vec<RouteMessage>,
}

impl Fib {
    async fn new() -> Self {
        let v = get_routes().await;
        Fib { routes: v }
    }
    async fn refresh(&mut self) {
        let v = get_routes().await;
        self.routes = v;
    }
}

async fn get_routes() -> Vec<RouteMessage> {
    let (connection, handle, _) = new_connection().unwrap();
    tokio::spawn(connection);
    let mut routes = handle.route().get(IpVersion::V4).execute();
    let mut v = vec![];
    while let Some(route) = routes.try_next().await.unwrap_or(None) {
        v.push(route);
    }
    v
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_refresh() {
        let f = tokio_test::block_on(Fib::new());
        let g = Fib::default();
        // tokio_test::block_on(g.refresh());
        assert_eq!(f, g);
    }
}
