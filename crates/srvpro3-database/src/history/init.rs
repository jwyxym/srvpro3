use super::entity::Entity;

use sea_orm::{
	ConnectionTrait, DatabaseConnection, DbErr, Schema, sea_query::TableCreateStatement,
};

pub async fn init(db: &DatabaseConnection) -> Result<(), DbErr> {
	let schema: Schema = Schema::new(db.get_database_backend());
	let statement: TableCreateStatement = schema
		.create_table_from_entity(Entity)
		.if_not_exists()
		.to_owned();
	db.execute(&statement).await?;
	Ok(())
}