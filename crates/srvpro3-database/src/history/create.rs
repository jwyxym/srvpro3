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
	player_c: Option<String>,
	player_d: Option<String>,
	deck_c: Option<String>,
	deck_d: Option<String>,
) -> Result<Model, Error> {
	let tag_fields = [player_c.is_some(), player_d.is_some(), deck_c.is_some(), deck_d.is_some()];
	anyhow::ensure!(tag_fields.iter().all(|value| *value) || tag_fields.iter().all(|value| !*value), "双打必须提供两名队友及其卡组");
	let now: i64 = Utc::now().timestamp();
	let deck_a: String = srvpro3_deck::encode(&deck_a)?;
	let deck_b: String = srvpro3_deck::encode(&deck_b)?;
	let deck_c = deck_c.as_deref().map(srvpro3_deck::encode).transpose()?;
	let deck_d = deck_d.as_deref().map(srvpro3_deck::encode).transpose()?;

	let match_records: ActiveModel = ActiveModel {
		player_a: Set(player_a),
		player_b: Set(player_b),
		deck_a: Set(deck_a),
		deck_b: Set(deck_b),
		player_c: Set(player_c),
		player_d: Set(player_d),
		deck_c: Set(deck_c),
		deck_d: Set(deck_d),
		winner_id: Set(winner_id),
		room_id: Set(room_id),
		replay: Set(replay),
		created_at: Set(now),
		..Default::default()
	};

	let result: Model = match_records.insert(db).await?;
	clear!("history:*");
	super::events::publish(super::events::Event::Add(result.clone()));
	Ok(result)
}
