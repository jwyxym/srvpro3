use super::{
	entity::{ActiveModel, Entity, Model},
	cache::clear
};
use sea_orm::{ActiveModelTrait, DatabaseConnection, DbErr, EntityTrait, Set};

pub async fn update(
	db: &DatabaseConnection,
	id: i64,
	winner_id: String,
	replay: String,
) -> Result<Model, DbErr> {
	let mut record: ActiveModel = Entity::find_by_id(id)
		.one(db)
		.await?
		.ok_or(DbErr::RecordNotFound("对局不存在".into()))?
		.into();

	record.winner_id = Set(Some(winner_id));
	record.replay = Set(Some(replay));

	let updated: Model = record.update(db).await?;
	let key: String = format!("history:name:{}*", updated.player_a);
	clear!(&key);
	let key: String = format!("history:name:{}*", updated.player_b);
	clear!(&key);
	let key: String = format!("history:room:{}*", updated.room_id);
	clear!(&key);
	let key: String = format!("history:id:{}*", updated.id);
	clear!(&key);
	clear!("history:all:*");

	Ok(updated)
}
