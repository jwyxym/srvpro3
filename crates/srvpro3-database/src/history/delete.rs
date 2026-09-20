use super::{
	entity::{Column, Entity},
	cache::clear
};
use sea_orm::{ColumnTrait, DatabaseConnection, DeleteResult, EntityTrait, QueryFilter};
use anyhow::Error;
use super::events::{self, Deleted, Event};

pub async fn all(db: &DatabaseConnection) -> Result<DeleteResult, Error> {
	let result: DeleteResult = Entity::delete_many().exec(db).await?;
	clear!("history:*");
	if result.rows_affected > 0 {
		events::publish(Event::Delete(Deleted { id: None, room_id: None, all: true, rows_affected: result.rows_affected }));
	}
	Ok(result)
}

pub async fn by_id(db: &DatabaseConnection, id: i64) -> Result<DeleteResult, Error> {
	let result: DeleteResult = Entity::delete_by_id(id).exec(db).await?;
	clear!("history:*");
	if result.rows_affected > 0 {
		events::publish(Event::Delete(Deleted { id: Some(id), room_id: None, all: false, rows_affected: result.rows_affected }));
	}
	Ok(result)
}

pub async fn by_room(db: &DatabaseConnection, room_id: String) -> Result<DeleteResult, Error> {
	let result: DeleteResult = Entity::delete_many()
		.filter(Column::RoomId.eq(room_id.clone()))
		.exec(db)
		.await?;
	clear!("history:*");
	if result.rows_affected > 0 {
		events::publish(Event::Delete(Deleted { id: None, room_id: Some(room_id), all: false, rows_affected: result.rows_affected }));
	}
	Ok(result)
}
