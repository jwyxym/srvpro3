use super::{
	entity::{Column, Entity},
	cache::clear
};
use sea_orm::{ColumnTrait, DatabaseConnection, DeleteResult, EntityTrait, QueryFilter};
use anyhow::Error;

pub async fn all(db: &DatabaseConnection) -> Result<DeleteResult, Error> {
	let result: DeleteResult = Entity::delete_many().exec(db).await?;
	clear!("history:*");
	Ok(result)
}

pub async fn by_id(db: &DatabaseConnection, id: i64) -> Result<DeleteResult, Error> {
	let result: DeleteResult = Entity::delete_by_id(id).exec(db).await?;
	clear!("history:*");
	Ok(result)
}

pub async fn by_room(db: &DatabaseConnection, room_id: String) -> Result<DeleteResult, Error> {
	let result: DeleteResult = Entity::delete_many()
		.filter(Column::RoomId.eq(room_id))
		.exec(db)
		.await?;
	clear!("history:*");
	Ok(result)
}