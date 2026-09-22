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
	// 双打队友也可以按名称查询历史，需要一并失效。
	for name in [&updated.player_c, &updated.player_d].into_iter().flatten() {
		let key = format!("history:name:{name}*");
		clear!(&key);
	}
	let key: String = format!("history:name:{}*", updated.player_a);
	clear!(&key);
	let key: String = format!("history:name:{}*", updated.player_b);
	clear!(&key);
	let key: String = format!("history:room:{}*", updated.room_id);
	clear!(&key);
	let key: String = format!("history:id:{}*", updated.id);
	clear!(&key);
	clear!("history:all:*");

	super::events::publish(super::events::Event::Update(updated.clone()));
	Ok(updated)
}
