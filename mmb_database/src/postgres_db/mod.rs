pub mod events;
pub mod migrator;

use anyhow::Result;
use tokio_postgres::tls::NoTlsStream;
use tokio_postgres::{Client as PgClient, Connection as PgConnection, NoTls, Socket};

pub struct Client(PgClient);

pub struct Connection(PgConnection<Socket, NoTlsStream>);

impl Connection {
    pub async fn handle(self) -> Result<()> {
        self.0.await.map_err(|e| e.into())
    }
}

pub async fn connect(database_url: &str) -> Result<(Client, Connection)> {
    // TODO enable tls
    let (client, connection) = tokio_postgres::connect(database_url, NoTls).await?;
    Ok((Client(client), Connection(connection)))
}
