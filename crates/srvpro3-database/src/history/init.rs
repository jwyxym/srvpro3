use super::entity::Entity;

use sea_orm::{
	ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Schema, Statement, sea_query::{TableCreateStatement, Table, ColumnDef, Alias},
};

pub async fn init(db: &DatabaseConnection) -> Result<(), DbErr> {
	let schema: Schema = Schema::new(db.get_database_backend());
	let statement: TableCreateStatement = schema
		.create_table_from_entity(Entity)
		.if_not_exists()
		.to_owned();
	db.execute(&statement).await?;
	// CREATE TABLE IF NOT EXISTS 不会更新旧表；逐列补齐，允许中断后重新启动。
	let backend = db.get_database_backend();
	let (sql, field) = match backend {
		DbBackend::Sqlite => ("PRAGMA table_info(history)", "name"),
		DbBackend::MySql => ("SELECT COLUMN_NAME AS column_name FROM information_schema.columns WHERE table_schema = DATABASE() AND table_name = 'history'", "column_name"),
		DbBackend::Postgres => ("SELECT column_name FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = 'history'", "column_name"),
		_ => return Err(DbErr::Custom("不支持此数据库的历史记录迁移".into())),
	};
	let columns = db.query_all_raw(Statement::from_string(backend, sql)).await?
		.into_iter().map(|row| row.try_get::<String>("", field)).collect::<Result<Vec<_>, _>>()?;
	for name in ["player_c", "player_d", "deck_c", "deck_d"] {
		if columns.iter().any(|column| column == name) { continue; }
		let mut column = ColumnDef::new(Alias::new(name));
		if name.starts_with("deck_") { column.text(); } else { column.string(); }
		column.null();
		let alter = Table::alter().table(Alias::new("history")).add_column(column).to_owned();
		db.execute(&alter).await?;
	}
	Ok(())
}
