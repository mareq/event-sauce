//! # Event sauce SQLX storage backend
//!
//! [![Build Status](https://circleci.com/gh/jamwaffles/event-sauce/tree/master.svg?style=shield)](https://circleci.com/gh/jamwaffles/event-sauce/tree/master)
//! [![Docs.rs](https://docs.rs/event-sauce-storage-sqlx/badge.svg)](https://docs.rs/event-sauce-storage-sqlx)
//!
//! [sqlx](https://crates.io/crates/sqlx) storage adapter for event-sauce-storage-sqlx.
//!
//! ## Features
//!
//! - `with-postgres` (enabled by default) - Enable support for Postgres databases by exposing the `SqlxPgStore` storage adapter.

#![deny(missing_docs)]
#![deny(intra_doc_link_resolution_failure)]

use event_sauce::DeleteBuilderPersist;
use event_sauce::StorageBackendTransaction;
use event_sauce::StorageBuilderPersist;
// use event_sauce::StoreToTransaction;
use event_sauce::{
    DBEvent, Deletable, DeleteBuilder, EventData, Persistable, StorageBackend, StorageBuilder,
};
use sqlx::pool::PoolConnection;
use sqlx::PgConnection;
use sqlx::Transaction;
use sqlx::{postgres::PgQueryAs, PgPool};
use std::convert::TryInto;

/// [sqlx](https://docs.rs/sqlx)-based Postgres backing store
#[derive(Debug, Clone)]
pub struct SqlxPgStore {
    /// sqlx [`PgPool`](sqlx::PgPool) to communicate with the database
    pub pool: PgPool,
}

impl SqlxPgStore {
    /// Create a new transaction
    pub async fn transaction(&self) -> Result<SqlxPgStoreTransaction, sqlx::Error> {
        let tx = self.pool.begin().await?;

        Ok(SqlxPgStoreTransaction(tx))
    }
}

#[async_trait::async_trait]
impl StorageBackend for SqlxPgStore {
    type Error = sqlx::Error;
    type Transaction = SqlxPgStoreTransaction;
}

/// TODO: Docs
pub struct SqlxPgStoreTransaction(Transaction<PoolConnection<PgConnection>>);

impl SqlxPgStoreTransaction {
    /// TODO: Docs
    pub fn get(&mut self) -> &mut Transaction<PoolConnection<PgConnection>> {
        &mut self.0
    }

    /// TODO: Docs
    pub async fn commit(self) -> Result<(), sqlx::Error> {
        self.0.commit().await?;

        Ok(())
    }
}

impl StorageBackendTransaction for SqlxPgStoreTransaction {
    type Error = sqlx::Error;
}

impl SqlxPgStore {
    /// Create a new backing store instance with a given [`PgPool`](sqlx::PgPool)
    pub async fn new(pool: PgPool) -> Result<Self, sqlx::Error> {
        Self::create_events_table(&pool).await?;

        Ok(Self { pool })
    }

    async fn create_events_table(pool: &PgPool) -> Result<(), sqlx::Error> {
        let mut tx = pool.begin().await?;

        sqlx::query(r#"create extension if not exists "uuid-ossp";"#)
            .execute(&mut tx)
            .await?;

        sqlx::query(r#"
            create table if not exists events(
                id uuid primary key,
                sequence_number serial,
                event_type varchar(64) not null,
                entity_type varchar(64) not null,
                entity_id uuid not null,
                -- This field is null if the event is purged, in such case purged_at and purger_id should be populated.
                data jsonb,
                session_id uuid null,
                created_at timestamp with time zone not null,
                purger_id uuid null,
                purged_at timestamp with time zone null
            );
        "#).execute(&mut tx).await?;

        tx.commit().await?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl Persistable<SqlxPgStoreTransaction, DBEvent> for DBEvent {
    async fn persist(self, store: &mut SqlxPgStoreTransaction) -> Result<Self, sqlx::Error> {
        let saved: Self = sqlx::query_as(
            r#"insert into events (
                id,
                event_type,
                entity_type,
                entity_id,
                data,
                session_id,
                created_at
            ) values (
                $1,
                $2,
                $3,
                $4,
                $5,
                $6,
                $7
            )
            on conflict (id)
            do update set
            data = excluded.data
            returning *"#,
        )
        .bind(self.id)
        .bind(self.event_type)
        .bind(self.entity_type)
        .bind(self.entity_id)
        .bind(self.data)
        .bind(self.session_id)
        .bind(self.created_at)
        .fetch_one(store.get())
        .await?;

        log::trace!("Persisted event {}: {:?}", saved.id, saved);

        Ok(saved)
    }
}

// #[async_trait::async_trait]
// impl StoreToTransaction for SqlxPgStore {
//     type Error = sqlx::Error;
//     type Transaction = SqlxPgStoreTransaction;

//     async fn transaction(&self) -> Result<Self::Transaction, Self::Error> {
//         let tx = self.pool.begin().await?;

//         Ok(SqlxPgStoreTransaction(tx))
//     }
// }

// // Transaction impl
// #[async_trait::async_trait]
// impl<E, ED> Persistable<SqlxPgStoreTransaction, E> for StorageBuilder<E, ED>
// where
//     ED: EventData + Send,
//     E: Persistable<SqlxPgStoreTransaction, E> + Send,
// {
//     async fn persist(self, tx: &mut SqlxPgStoreTransaction) -> Result<E, sqlx::Error> {
//         // TODO: Enum error type to handle this unwrap
//         let db_event: DBEvent = self
//             .event
//             .try_into()
//             .expect("Failed to convert Event into DBEvent");

//         db_event.persist(tx).await?;

//         self.entity.persist(tx).await
//     }
// }

// #[async_trait::async_trait]
// impl<E, ED> StorePersistThing<SqlxPgStore, E> for StorageBuilder<E, ED>
// where
//     ED: EventData + Send,
//     E: Persistable<SqlxPgStoreTransaction, E> + Send,
// {
//     type Error = sqlx::Error;

//     async fn persist(self, store: &SqlxPgStore) -> Result<E, Self::Error> {
//         let mut tx: SqlxPgStoreTransaction = store.transaction().await?;

//         // let new = Persistable::persist(self, &mut tx).await?;
//         // TODO: Enum error type to handle this unwrap
//         let db_event: DBEvent = self
//             .event
//             .try_into()
//             .expect("Failed to convert Event into DBEvent");

//         db_event.persist(&mut tx).await?;

//         let new = self.entity.persist(&mut tx).await?;

//         tx.0.commit().await?;

//         Ok(new)
//     }
// }

// // Transaction impl
// #[async_trait::async_trait]
// impl<Ent, ED> Deletable<SqlxPgStoreTransaction> for DeleteBuilder<Ent, ED>
// where
//     ED: EventData + Send,
//     Ent: Deletable<SqlxPgStoreTransaction> + Send,
// {
//     async fn delete(self, tx: &mut SqlxPgStoreTransaction) -> Result<(), sqlx::Error> {
//         // TODO: Enum error type to handle this unwrap
//         let db_event: DBEvent = self
//             .event
//             .try_into()
//             .expect("Failed to convert Event into DBEvent");

//         db_event.persist(tx).await?;

//         println!("Event persisted");

//         self.entity.delete(tx).await
//     }
// }

// #[async_trait::async_trait]
// impl<E, ED> StorePersistThing<SqlxPgStore, ()> for DeleteBuilder<E, ED>
// where
//     ED: EventData + Send,
//     E: Deletable<SqlxPgStoreTransaction> + Send,
// {
//     type Error = sqlx::Error;

//     async fn persist(self, store: &SqlxPgStore) -> Result<(), Self::Error> {
//         let mut tx: SqlxPgStoreTransaction = store.transaction().await?;

//         Deletable::delete(self, &mut tx).await?;

//         tx.0.commit().await?;

//         Ok(())
//     }
// }

#[async_trait::async_trait]
impl<E, ED> StorageBuilderPersist<SqlxPgStore, E> for StorageBuilder<E, ED>
where
    E: Persistable<SqlxPgStoreTransaction> + Send,
    ED: EventData + Send,
{
    async fn stage_persist(self, tx: &mut SqlxPgStoreTransaction) -> Result<E, sqlx::Error> {
        // TODO: Enum error type to handle this unwrap
        let db_event: DBEvent = self
            .event
            .try_into()
            .expect("Failed to convert Event into DBEvent");

        db_event.persist(tx).await?;

        self.entity.persist(tx).await
    }

    async fn persist(self, store: &SqlxPgStore) -> Result<E, sqlx::Error> {
        let mut tx = store.transaction().await?;

        // TODO: Enum error type to handle this unwrap
        let db_event: DBEvent = self
            .event
            .try_into()
            .expect("Failed to convert Event into DBEvent");

        db_event.persist(&mut tx).await?;

        let new = self.entity.persist(&mut tx).await?;

        tx.commit().await?;

        Ok(new)
    }
}

#[async_trait::async_trait]
impl<E, ED> DeleteBuilderPersist<SqlxPgStore> for DeleteBuilder<E, ED>
where
    E: Deletable<SqlxPgStoreTransaction> + Send,
    ED: EventData + Send,
{
    async fn stage_delete(self, tx: &mut SqlxPgStoreTransaction) -> Result<(), sqlx::Error> {
        // TODO: Enum error type to handle this unwrap
        let db_event: DBEvent = self
            .event
            .try_into()
            .expect("Failed to convert Event into DBEvent");

        db_event.persist(tx).await?;

        self.entity.delete(tx).await?;

        Ok(())
    }

    async fn delete(self, store: &SqlxPgStore) -> Result<(), sqlx::Error> {
        let mut tx = store.transaction().await?;

        // TODO: Enum error type to handle this unwrap
        let db_event: DBEvent = self
            .event
            .try_into()
            .expect("Failed to convert Event into DBEvent");

        db_event.persist(&mut tx).await?;

        self.entity.delete(&mut tx).await?;

        tx.commit().await
    }
}
