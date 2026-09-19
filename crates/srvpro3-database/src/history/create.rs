use super::{
	entity::{ActiveModel, Model},
	cache::clear
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};
use anyhow::Error;

pub async fn create(
	db: &DatabaseConnection,
	player_a: String,
	player_b: String,
	deck_a: String,
	deck_b: String,
	winner_id: Option<String>,
	room_id: String,
	replay: Option<String>,
) -> Result<Model, Error> {
	let now: i64 = Utc::now().timestamp();
	let deck_a: String = srvpro3_deck::encode(&deck_a)?;
	let deck_b: String = srvpro3_deck::encode(&deck_b)?;

	let match_records: ActiveModel = ActiveModel {
		player_a: Set(player_a),
		player_b: Set(player_b),
		deck_a: Set(deck_a),
		deck_b: Set(deck_b),
		winner_id: Set(winner_id),
		room_id: Set(room_id),
		replay: Set(replay),
		created_at: Set(now),
		..Default::default()
	};

	let result: Model = match_records.insert(db).await?;
	clear!("history:*");
	Ok(result)
}