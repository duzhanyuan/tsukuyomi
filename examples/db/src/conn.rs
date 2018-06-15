use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::sqlite::SqliteConnection;
use failure::Error;
use futures::future::poll_fn;
use futures::{Async, Future, IntoFuture};
use tokio_threadpool::blocking;

use tsukuyomi::app::AppState;
use tsukuyomi::Context;

pub type ConnPool = Pool<ConnectionManager<SqliteConnection>>;
pub type Conn = PooledConnection<ConnectionManager<SqliteConnection>>;

pub fn init_pool(database_url: String) -> Result<ConnPool, Error> {
    let manager = ConnectionManager::<SqliteConnection>::new(database_url);
    let pool = Pool::builder().max_size(15).build(manager)?;
    Ok(pool)
}

// TODO: take the reference to AppState from Context directly.
pub fn get_conn(_cx: &Context) -> impl Future<Item = Conn, Error = Error> + Send + 'static {
    AppState::with(|s| s.state::<ConnPool>().cloned())
        .ok_or_else(|| format_err!("The connection pool is not exist"))
        .into_future()
        .and_then(|pool| {
            poll_fn(move || {
                try_ready!(blocking(|| pool.get()))
                    .map(Async::Ready)
                    .map_err(Into::into)
            })
        })
}
