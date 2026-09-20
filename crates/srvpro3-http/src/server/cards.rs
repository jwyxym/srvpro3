use axum::{Json, http::StatusCode};
use serde::Serialize;

#[derive(Serialize)]
pub struct Counts {
	pub cards: usize,
	pub packs: usize,
	pub lflists: usize,
}

pub async fn get() -> Result<Json<Counts>, (StatusCode, &'static str)> {
	let snapshot = srvpro3_cards::get()
		.map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "卡片数据尚未加载"))?;
	Ok(Json(Counts {
		cards: snapshot.cards.len(),
		packs: snapshot.packs.len(),
		lflists: snapshot.lflists.len(),
	}))
}
