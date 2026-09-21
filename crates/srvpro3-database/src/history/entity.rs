use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "history")]
pub struct Model {
	#[sea_orm(primary_key, auto_increment = true)]
	pub id: i64, // 对局id
	pub player_a: String,       // 玩家A
	pub player_b: String,       // 玩家B
	pub deck_a: String,         // 玩家A卡组
	pub deck_b: String,         // 玩家B卡组
	pub winner_id: Option<String>, // 胜利者名称；无胜者时为空
	pub room_id: String,        // 房间
	#[sea_orm(column_type = "Text")]
	pub replay: Option<String>, // 上游 .yrp3d 文件：yrp3d:v1: + gzip/Base64
	pub created_at: i64,        // 时间戳
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
