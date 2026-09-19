use crate::connect;
use super::{
	entity::{Column, Entity, Model},
	cache::{read, write}
};
use sea_orm::{
	ColumnTrait, Condition, DatabaseConnection, EntityTrait, Paginator, PaginatorTrait,
	QueryFilter, QueryOrder, SelectModel,
};
use anyhow::Error;

pub async fn by_id(id: i64) -> Result<Option<Model>, Error> {
	let key: String = format!("history:id:{id}");

	if let Some(model) = read!(&key, Model) {
		return Ok(Some(model));
	}
	let db: DatabaseConnection = connect::get::db()?;
	let result: Option<Model> = Entity::find_by_id(id)
		.one(&db).await?;
	if let Some(result) = result {
		write!(key, result);
		Ok(Some(result))
	} else {
		Ok(None)
	}
}

pub async fn by_name(
	name: &str,
	page: u64,
	page_size: u64,
) -> Result<(Vec<Model>, u64), Error> {
	let key: String = format!("history:name:{name}-{page}-{page_size}");

	if let Some(result) = read!(&key, (Vec<Model>, u64)) {
		return Ok(result);
	}
	let db: DatabaseConnection = connect::get::db()?;
	let paginator: Paginator<'_, DatabaseConnection, SelectModel<Model>> = Entity::find()
		.filter(
			Condition::any()
				.add(Column::PlayerA.eq(name.to_owned()))
				.add(Column::PlayerB.eq(name.to_owned())),
		)
		.order_by_desc(Column::CreatedAt)
		.paginate(&db, page_size);


	let total: u64 = paginator.num_items().await?;
	let list: Vec<Model> = paginator.fetch_page(page).await?;
	let result: (Vec<Model>, u64) = (list, total);

	write!(key, result);

	Ok(result)
}

pub async fn all(
	page: u64,
	page_size: u64,
) -> Result<(Vec<Model>, u64), Error> {
	let key: String = format!("history:all:{page}-{page_size}");

	if let Some(result) = read!(&key, (Vec<Model>, u64)) {
		return Ok(result);
	}

	let db: DatabaseConnection = connect::get::db()?;
	let paginator: Paginator<'_, DatabaseConnection, SelectModel<Model>> = Entity::find()
		.order_by_desc(Column::CreatedAt)
		.paginate(&db, page_size);

	let total: u64 = paginator.num_items().await?;
	let list: Vec<Model> = paginator.fetch_page(page).await?;
	let result: (Vec<Model>, u64) = (list, total);

	write!(key, result);

	Ok(result)
}

pub async fn by_room(
	room_id: &str,
	page: u64,
	page_size: u64,
) -> Result<(Vec<Model>, u64), Error> {
	let key: String = format!("history:room:{room_id}-{page}-{page_size}");

	if let Some(result) = read!(&key, (Vec<Model>, u64)) {
		return Ok(result);
	}

	let db: DatabaseConnection = connect::get::db()?;
	let paginator: Paginator<'_, DatabaseConnection, SelectModel<Model>> = Entity::find()
		.filter(Column::RoomId.eq(room_id.to_owned()))
		.order_by_desc(Column::CreatedAt)
		.paginate(&db, page_size);

	let total: u64 = paginator.num_items().await?;
	let list: Vec<Model> = paginator.fetch_page(page).await?;
	let result: (Vec<Model>, u64) = (list, total);

	write!(key, result);

	Ok(result)
}
