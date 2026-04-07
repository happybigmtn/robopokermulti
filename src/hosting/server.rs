use super::*;
use crate::gameplay::TableConfig;
use actix_cors::Cors;
use actix_web::App;
use actix_web::HttpRequest;
use actix_web::HttpResponse;
use actix_web::HttpServer;
use actix_web::Responder;
use actix_web::middleware::Logger;
use actix_web::web;
use serde::Deserialize;

#[derive(Deserialize)]
struct StartRoom {
    seat_count: Option<usize>,
    small_blind: Option<i16>,
    big_blind: Option<i16>,
    ante: Option<i16>,
    starting_stack: Option<i16>,
    owned_seat: Option<usize>,
    owned_seats: Option<Vec<usize>>,
    open_seats: Option<Vec<usize>>,
}

#[derive(Deserialize)]
struct RoomAccess {
    seat: usize,
    generation: u64,
    token: String,
}

#[derive(Deserialize)]
struct ClaimSeat {
    seat: usize,
    generation: u64,
    token: String,
    target_seat: usize,
}

#[derive(Deserialize)]
struct JoinSeat {
    target_seat: usize,
}

pub struct Server;

impl Server {
    pub async fn run() -> Result<(), std::io::Error> {
        let state = web::Data::new(Casino::default());
        log::info!("starting hosting server");
        HttpServer::new(move || {
            App::new()
                .wrap(Logger::new("%r %s %Ts"))
                .wrap(
                    Cors::default()
                        .allow_any_origin()
                        .allow_any_method()
                        .allow_any_header(),
                )
                .app_data(state.clone())
                .route("/start", web::post().to(start))
                .route("/invite/{invitation_id}", web::get().to(invite))
                .route("/lobby/{room_id}", web::get().to(lobby))
                .route("/room/{room_id}", web::get().to(room_status))
                .route("/enter/{room_id}", web::get().to(enter))
                .route("/leave/{room_id}", web::post().to(leave))
                .route("/close/{room_id}", web::post().to(close))
                .route("/claim/{room_id}", web::post().to(claim))
                .route("/join/{room_id}", web::post().to(join))
        })
        .workers(4)
        .bind(std::env::var("BIND_ADDR").expect("BIND_ADDR must be set"))?
        .run()
        .await
    }
}

fn table_json(table: TableConfig) -> serde_json::Value {
    serde_json::json!({
        "seat_count": table.seat_count,
        "small_blind": table.small_blind,
        "big_blind": table.big_blind,
        "ante": table.ante,
        "starting_stack": table.starting_stack,
    })
}

fn seat_session_json(room_id: RoomId, access: &SeatAccess) -> serde_json::Value {
    serde_json::json!({
        "seat": access.seat,
        "generation": access.generation,
        "token": access.token,
        "status_path": format!("/room/{}", room_id),
        "enter_path": format!("/enter/{}", room_id),
        "leave_path": format!("/leave/{}", room_id),
        "close_path": format!("/close/{}", room_id),
        "query": {
            "seat": access.seat,
            "generation": access.generation,
            "token": access.token,
        },
    })
}

fn seat_sessions_json(room_id: RoomId, seat_accesses: &[SeatAccess]) -> Vec<serde_json::Value> {
    seat_accesses
        .iter()
        .map(|access| seat_session_json(room_id, access))
        .collect()
}

fn join_option_json(room_id: RoomId, seat: usize) -> serde_json::Value {
    serde_json::json!({
        "seat": seat,
        "join_path": format!("/join/{}", room_id),
        "body": {
            "target_seat": seat,
        },
    })
}

fn join_options_json(room_id: RoomId, open_seats: &[usize]) -> Vec<serde_json::Value> {
    open_seats
        .iter()
        .copied()
        .map(|seat| join_option_json(room_id, seat))
        .collect()
}

fn resume_json(room_id: RoomId, seat_accesses: &[SeatAccess]) -> serde_json::Value {
    serde_json::json!({
        "room_id": room_id,
        "seat_sessions": seat_sessions_json(room_id, seat_accesses),
    })
}

fn invitation_json(
    room_id: RoomId,
    invitation_id: &str,
    open_seats: &[usize],
) -> serde_json::Value {
    serde_json::json!({
        "room_id": room_id,
        "invitation_id": invitation_id,
        "invite_path": format!("/invite/{}", invitation_id),
        "lobby_path": format!("/lobby/{}", room_id),
        "join_options": join_options_json(room_id, open_seats),
    })
}

fn room_access_error_response(err: RoomAccessError) -> HttpResponse {
    match err {
        RoomAccessError::RoomNotFound => HttpResponse::NotFound().body(err.to_string()),
        RoomAccessError::InvalidToken
        | RoomAccessError::WrongGeneration { .. }
        | RoomAccessError::WrongSeat { .. } => HttpResponse::Unauthorized().body(err.to_string()),
    }
}

async fn start(casino: web::Data<Casino>, req: Option<web::Json<StartRoom>>) -> impl Responder {
    let (config, owned_seats, open_seats) = match build_start_config(req.as_deref()) {
        Ok(config) => config,
        Err(err) => return HttpResponse::BadRequest().body(err),
    };

    match casino.start(config, owned_seats, open_seats).await {
        Ok(start) => {
            let open_seats = start
                .seat_assignments
                .iter()
                .filter(|assignment| assignment.kind == SeatKind::Open)
                .map(|assignment| assignment.seat)
                .collect::<Vec<_>>();
            HttpResponse::Ok().json(serde_json::json!({
                "room_id": start.room_id,
                "invitation_id": start.invitation_id,
                "owned_seats": start.owned_seats,
                "seat_accesses": start.seat_accesses,
                "seat_sessions": seat_sessions_json(start.room_id, &start.seat_accesses),
                "resume": resume_json(start.room_id, &start.seat_accesses),
                "seat_assignments": start.seat_assignments,
                "seat_roles": super::seat_roles_from_assignments(&start.seat_assignments),
                "open_seats": open_seats,
                "join_options": join_options_json(start.room_id, &open_seats),
                "invitation": invitation_json(start.room_id, &start.invitation_id, &open_seats),
                "table": table_json(start.table),
            }))
        }
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

fn claim_seat_error_response(err: ClaimSeatError) -> HttpResponse {
    match err {
        ClaimSeatError::Access(err) => room_access_error_response(err),
        ClaimSeatError::SeatUnavailable { .. } => HttpResponse::Conflict().body(err.to_string()),
        ClaimSeatError::SeatOutOfRange { .. } => HttpResponse::BadRequest().body(err.to_string()),
    }
}

async fn claim(
    casino: web::Data<Casino>,
    path: web::Path<RoomId>,
    req: web::Json<ClaimSeat>,
) -> impl Responder {
    let id = path.into_inner();
    match casino
        .claim_seat(id, req.seat, req.generation, &req.token, req.target_seat)
        .await
    {
        Ok(access) => HttpResponse::Ok().json(serde_json::json!({
            "seat_access": access,
            "seat_session": seat_session_json(id, &access),
            "resume": resume_json(id, std::slice::from_ref(&access)),
        })),
        Err(err) => claim_seat_error_response(err),
    }
}

fn join_seat_error_response(err: JoinSeatError) -> HttpResponse {
    match err {
        JoinSeatError::RoomNotFound => HttpResponse::NotFound().body(err.to_string()),
        JoinSeatError::SeatUnavailable { .. } => HttpResponse::Conflict().body(err.to_string()),
        JoinSeatError::SeatOutOfRange { .. } => HttpResponse::BadRequest().body(err.to_string()),
    }
}

async fn join(
    casino: web::Data<Casino>,
    path: web::Path<RoomId>,
    req: web::Json<JoinSeat>,
) -> impl Responder {
    let id = path.into_inner();
    match casino.join_open_seat(id, req.target_seat).await {
        Ok(access) => HttpResponse::Ok().json(serde_json::json!({
            "seat_access": access,
            "seat_session": seat_session_json(id, &access),
            "resume": resume_json(id, std::slice::from_ref(&access)),
        })),
        Err(err) => join_seat_error_response(err),
    }
}

async fn leave(
    casino: web::Data<Casino>,
    path: web::Path<RoomId>,
    query: web::Query<RoomAccess>,
) -> impl Responder {
    let id = path.into_inner();
    match casino
        .leave_seat(id, query.seat, query.generation, &query.token)
        .await
    {
        Ok(snapshot) => {
            let open_seats = snapshot
                .seat_assignments
                .iter()
                .filter(|assignment| assignment.kind == SeatKind::Open)
                .map(|assignment| assignment.seat)
                .collect::<Vec<_>>();
            HttpResponse::Ok().json(serde_json::json!({
                "status": "left",
                "room_id": snapshot.room_id,
                "invitation_id": snapshot.invitation_id,
                "owned_seats": snapshot.owned_seats,
                "seat_accesses": snapshot.seat_accesses,
                "seat_sessions": seat_sessions_json(snapshot.room_id, &snapshot.seat_accesses),
                "resume": resume_json(snapshot.room_id, &snapshot.seat_accesses),
                "seat_assignments": snapshot.seat_assignments,
                "seat_roles": super::seat_roles_from_assignments(&snapshot.seat_assignments),
                "open_seats": open_seats,
                "join_options": join_options_json(snapshot.room_id, &open_seats),
                "invitation": invitation_json(
                    snapshot.room_id,
                    &snapshot.invitation_id,
                    &open_seats,
                ),
            }))
        }
        Err(LeaveSeatError::Access(err)) => room_access_error_response(err),
        Err(LeaveSeatError::SeatUnavailable { .. } | LeaveSeatError::SeatOutOfRange { .. }) => {
            HttpResponse::Conflict().body("seat cannot be left in its current state")
        }
    }
}

async fn close(
    casino: web::Data<Casino>,
    path: web::Path<RoomId>,
    query: web::Query<RoomAccess>,
) -> impl Responder {
    let id = path.into_inner();
    if let Err(err) = casino
        .authorize(id, query.seat, query.generation, &query.token)
        .await
    {
        return room_access_error_response(err);
    }
    match casino.close(id).await {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({ "status": "closed" })),
        Err(e) => HttpResponse::NotFound().body(e.to_string()),
    }
}

async fn room_status(
    casino: web::Data<Casino>,
    path: web::Path<RoomId>,
    query: web::Query<RoomAccess>,
) -> impl Responder {
    let id = path.into_inner();
    match casino
        .snapshot(id, query.seat, query.generation, &query.token)
        .await
    {
        Ok(snapshot) => {
            let open_seats = snapshot
                .seat_assignments
                .iter()
                .filter(|assignment| assignment.kind == SeatKind::Open)
                .map(|assignment| assignment.seat)
                .collect::<Vec<_>>();
            HttpResponse::Ok().json(serde_json::json!({
                "room_id": snapshot.room_id,
                "invitation_id": snapshot.invitation_id,
                "owned_seats": snapshot.owned_seats,
                "seat_accesses": snapshot.seat_accesses,
                "seat_sessions": seat_sessions_json(snapshot.room_id, &snapshot.seat_accesses),
                "resume": resume_json(snapshot.room_id, &snapshot.seat_accesses),
                "seat_assignments": snapshot.seat_assignments,
                "seat_roles": super::seat_roles_from_assignments(&snapshot.seat_assignments),
                "open_seats": open_seats,
                "join_options": join_options_json(snapshot.room_id, &open_seats),
                "invitation": invitation_json(
                    snapshot.room_id,
                    &snapshot.invitation_id,
                    &open_seats,
                ),
                "table": snapshot.table.map(table_json).unwrap_or(serde_json::Value::Null),
                "connected": snapshot.connected,
                "table_state": snapshot
                    .latest_table_state
                    .as_deref()
                    .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
                    .unwrap_or(serde_json::Value::Null),
                "decision": snapshot
                    .latest_decision
                    .as_deref()
                    .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
                    .unwrap_or(serde_json::Value::Null),
            }))
        }
        Err(err) => room_access_error_response(err),
    }
}

async fn lobby(casino: web::Data<Casino>, path: web::Path<RoomId>) -> impl Responder {
    let id = path.into_inner();
    match casino.public_snapshot(id).await {
        Ok(snapshot) => HttpResponse::Ok().json(serde_json::json!({
            "room_id": snapshot.room_id,
            "invitation_id": snapshot.invitation_id,
            "owned_seats": snapshot.owned_seats,
            "open_seats": snapshot.open_seats,
            "connected_seats": snapshot.connected_seats,
            "seat_assignments": snapshot.seat_assignments,
            "seat_roles": super::seat_roles_from_assignments(&snapshot.seat_assignments),
            "join_options": join_options_json(snapshot.room_id, &snapshot.open_seats),
            "invitation": invitation_json(
                snapshot.room_id,
                &snapshot.invitation_id,
                &snapshot.open_seats,
            ),
            "table": snapshot.table.map(table_json).unwrap_or(serde_json::Value::Null),
        })),
        Err(RoomAccessError::RoomNotFound) => HttpResponse::NotFound().body("room not found"),
        Err(
            RoomAccessError::InvalidToken
            | RoomAccessError::WrongGeneration { .. }
            | RoomAccessError::WrongSeat { .. },
        ) => HttpResponse::InternalServerError().body("unexpected room access error"),
    }
}

async fn invite(casino: web::Data<Casino>, path: web::Path<String>) -> impl Responder {
    let invitation_id = path.into_inner();
    match casino.public_snapshot_by_invitation(&invitation_id).await {
        Ok(snapshot) => HttpResponse::Ok().json(serde_json::json!({
            "room_id": snapshot.room_id,
            "invitation_id": snapshot.invitation_id,
            "owned_seats": snapshot.owned_seats,
            "open_seats": snapshot.open_seats,
            "connected_seats": snapshot.connected_seats,
            "seat_assignments": snapshot.seat_assignments,
            "seat_roles": super::seat_roles_from_assignments(&snapshot.seat_assignments),
            "join_options": join_options_json(snapshot.room_id, &snapshot.open_seats),
            "invitation": invitation_json(
                snapshot.room_id,
                &snapshot.invitation_id,
                &snapshot.open_seats,
            ),
            "table": snapshot.table.map(table_json).unwrap_or(serde_json::Value::Null),
        })),
        Err(RoomAccessError::RoomNotFound) => HttpResponse::NotFound().body("room not found"),
        Err(
            RoomAccessError::InvalidToken
            | RoomAccessError::WrongGeneration { .. }
            | RoomAccessError::WrongSeat { .. },
        ) => HttpResponse::InternalServerError().body("unexpected room access error"),
    }
}

async fn enter(
    casino: web::Data<Casino>,
    path: web::Path<RoomId>,
    query: web::Query<RoomAccess>,
    body: web::Payload,
    req: HttpRequest,
) -> impl Responder {
    let id = path.into_inner();
    if let Err(err) = casino
        .authorize(id, query.seat, query.generation, &query.token)
        .await
    {
        return room_access_error_response(err).map_into_right_body();
    }
    match actix_ws::handle(&req, body) {
        Ok((response, session, stream)) => match casino
            .bridge(id, query.seat, session, stream)
            .await
        {
            Ok(()) => response.map_into_left_body(),
            Err(BridgeError::RoomNotFound) => HttpResponse::NotFound()
                .body("room not found")
                .map_into_right_body(),
            Err(BridgeError::AlreadyConnected) => HttpResponse::Conflict()
                .body("room already has an active client connection")
                .map_into_right_body(),
            Err(BridgeError::CachedReplayFailed(message)) => HttpResponse::InternalServerError()
                .body(message)
                .map_into_right_body(),
        },
        Err(e) => HttpResponse::InternalServerError()
            .body(e.to_string())
            .map_into_right_body(),
    }
}

fn build_start_config(
    req: Option<&StartRoom>,
) -> Result<(TableConfig, Vec<usize>, Vec<usize>), String> {
    let Some(req) = req else {
        return Ok((TableConfig::heads_up(), vec![0], Vec::new()));
    };

    let any_set = req.seat_count.is_some()
        || req.small_blind.is_some()
        || req.big_blind.is_some()
        || req.ante.is_some()
        || req.starting_stack.is_some()
        || req.owned_seat.is_some()
        || req.owned_seats.is_some()
        || req.open_seats.is_some();
    if !any_set {
        return Ok((TableConfig::heads_up(), vec![0], Vec::new()));
    }

    let seat_count = req
        .seat_count
        .ok_or_else(|| "seat_count is required when providing room table config".to_string())?;
    let small_blind = req
        .small_blind
        .ok_or_else(|| "small_blind is required when providing room table config".to_string())?;
    let big_blind = req
        .big_blind
        .ok_or_else(|| "big_blind is required when providing room table config".to_string())?;
    let starting_stack = req
        .starting_stack
        .ok_or_else(|| "starting_stack is required when providing room table config".to_string())?;
    let ante = req.ante.unwrap_or(0);

    let config = TableConfig {
        seat_count,
        small_blind,
        big_blind,
        ante,
        starting_stack,
    };
    config.validate().map_err(str::to_string)?;
    if req.owned_seat.is_some() && req.owned_seats.is_some() {
        return Err("provide either owned_seat or owned_seats, not both".to_string());
    }
    let owned_seats = if let Some(owned_seats) = &req.owned_seats {
        owned_seats.clone()
    } else {
        vec![req.owned_seat.unwrap_or(0)]
    };
    if owned_seats.is_empty() {
        return Err("owned_seats must contain at least one seat".to_string());
    }
    let mut deduped = owned_seats.clone();
    deduped.sort_unstable();
    deduped.dedup();
    if deduped.len() != owned_seats.len() {
        return Err("owned_seats must not contain duplicates".to_string());
    }
    if let Some(seat) = owned_seats
        .iter()
        .copied()
        .find(|seat| *seat >= config.seat_count)
    {
        return Err(format!(
            "owned seat {} is out of range for seat_count {}",
            seat, config.seat_count
        ));
    }
    let open_seats = req.open_seats.clone().unwrap_or_default();
    let mut deduped_open = open_seats.clone();
    deduped_open.sort_unstable();
    deduped_open.dedup();
    if deduped_open.len() != open_seats.len() {
        return Err("open_seats must not contain duplicates".to_string());
    }
    if let Some(seat) = open_seats
        .iter()
        .copied()
        .find(|seat| *seat >= config.seat_count)
    {
        return Err(format!(
            "open seat {} is out of range for seat_count {}",
            seat, config.seat_count
        ));
    }
    if let Some(seat) = open_seats
        .iter()
        .copied()
        .find(|seat| owned_seats.contains(seat))
    {
        return Err(format!("seat {} cannot be both owned and open", seat));
    }
    Ok((config, owned_seats, open_seats))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_start_config_defaults_to_heads_up() {
        let (config, owned_seats, open_seats) =
            build_start_config(None).expect("default start config");

        assert_eq!(config, TableConfig::heads_up());
        assert_eq!(owned_seats, vec![0]);
        assert!(open_seats.is_empty());
    }

    #[test]
    fn build_start_config_accepts_multiway_values() {
        let req = StartRoom {
            seat_count: Some(6),
            small_blind: Some(2),
            big_blind: Some(4),
            ante: Some(1),
            starting_stack: Some(300),
            owned_seat: Some(2),
            owned_seats: None,
            open_seats: None,
        };

        let (config, owned_seats, open_seats) =
            build_start_config(Some(&req)).expect("multiway room config");

        assert_eq!(config.seat_count, 6);
        assert_eq!(config.small_blind, 2);
        assert_eq!(config.big_blind, 4);
        assert_eq!(config.ante, 1);
        assert_eq!(config.starting_stack, 300);
        assert_eq!(owned_seats, vec![2]);
        assert!(open_seats.is_empty());
    }

    #[test]
    fn build_start_config_requires_complete_partial_config() {
        let req = StartRoom {
            seat_count: Some(4),
            small_blind: None,
            big_blind: Some(4),
            ante: None,
            starting_stack: Some(200),
            owned_seat: None,
            owned_seats: None,
            open_seats: None,
        };

        let err = build_start_config(Some(&req)).expect_err("partial config should fail");

        assert!(err.contains("small_blind"));
    }

    #[test]
    fn build_start_config_rejects_out_of_range_owned_seat() {
        let req = StartRoom {
            seat_count: Some(4),
            small_blind: Some(2),
            big_blind: Some(4),
            ante: Some(0),
            starting_stack: Some(200),
            owned_seat: Some(4),
            owned_seats: None,
            open_seats: None,
        };

        let err = build_start_config(Some(&req)).expect_err("out of range owned seat should fail");

        assert!(err.contains("owned seat"));
    }

    #[test]
    fn build_start_config_accepts_multiple_owned_seats() {
        let req = StartRoom {
            seat_count: Some(6),
            small_blind: Some(2),
            big_blind: Some(4),
            ante: Some(1),
            starting_stack: Some(300),
            owned_seat: None,
            owned_seats: Some(vec![1, 4]),
            open_seats: None,
        };

        let (config, owned_seats, open_seats) =
            build_start_config(Some(&req)).expect("multi-seat config");

        assert_eq!(config.seat_count, 6);
        assert_eq!(owned_seats, vec![1, 4]);
        assert!(open_seats.is_empty());
    }

    #[test]
    fn build_start_config_rejects_duplicate_owned_seats() {
        let req = StartRoom {
            seat_count: Some(6),
            small_blind: Some(2),
            big_blind: Some(4),
            ante: Some(1),
            starting_stack: Some(300),
            owned_seat: None,
            owned_seats: Some(vec![1, 1]),
            open_seats: None,
        };

        let err = build_start_config(Some(&req)).expect_err("duplicate seats should fail");

        assert!(err.contains("duplicates"));
    }

    #[test]
    fn build_start_config_rejects_mixed_owned_seat_inputs() {
        let req = StartRoom {
            seat_count: Some(6),
            small_blind: Some(2),
            big_blind: Some(4),
            ante: Some(1),
            starting_stack: Some(300),
            owned_seat: Some(1),
            owned_seats: Some(vec![2, 4]),
            open_seats: None,
        };

        let err = build_start_config(Some(&req)).expect_err("mixed owned seat inputs should fail");

        assert!(err.contains("either owned_seat or owned_seats"));
    }

    #[test]
    fn build_start_config_accepts_open_seats() {
        let req = StartRoom {
            seat_count: Some(6),
            small_blind: Some(2),
            big_blind: Some(4),
            ante: Some(1),
            starting_stack: Some(300),
            owned_seat: None,
            owned_seats: Some(vec![1]),
            open_seats: Some(vec![3, 5]),
        };

        let (_config, owned_seats, open_seats) =
            build_start_config(Some(&req)).expect("open seats config");

        assert_eq!(owned_seats, vec![1]);
        assert_eq!(open_seats, vec![3, 5]);
    }

    #[test]
    fn build_start_config_rejects_overlapping_open_and_owned_seats() {
        let req = StartRoom {
            seat_count: Some(6),
            small_blind: Some(2),
            big_blind: Some(4),
            ante: Some(1),
            starting_stack: Some(300),
            owned_seat: None,
            owned_seats: Some(vec![1, 4]),
            open_seats: Some(vec![4, 5]),
        };

        let err = build_start_config(Some(&req)).expect_err("overlap should fail");

        assert!(err.contains("both owned and open"));
    }

    #[test]
    fn room_access_holds_token_value() {
        let access = RoomAccess {
            seat: 0,
            generation: 3,
            token: "test-token".to_string(),
        };

        assert_eq!(access.seat, 0);
        assert_eq!(access.generation, 3);
        assert_eq!(access.token, "test-token");
    }

    #[test]
    fn room_status_nulls_missing_cached_json() {
        let table_state = Option::<&str>::None
            .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
            .unwrap_or(serde_json::Value::Null);
        let decision = Option::<&str>::None
            .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
            .unwrap_or(serde_json::Value::Null);

        assert_eq!(table_state, serde_json::Value::Null);
        assert_eq!(decision, serde_json::Value::Null);
    }

    #[test]
    fn room_access_error_response_uses_expected_status_codes() {
        assert_eq!(
            room_access_error_response(RoomAccessError::RoomNotFound).status(),
            actix_web::http::StatusCode::NOT_FOUND
        );
        assert_eq!(
            room_access_error_response(RoomAccessError::InvalidToken).status(),
            actix_web::http::StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            room_access_error_response(RoomAccessError::WrongGeneration {
                seat: 2,
                expected: 3,
                requested: 1,
            })
            .status(),
            actix_web::http::StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            room_access_error_response(RoomAccessError::WrongSeat {
                owned_seats: vec![0, 2],
                requested: 1,
            })
            .status(),
            actix_web::http::StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn seat_session_json_includes_generation_and_query_shape() {
        let access = SeatAccess {
            seat: 2,
            generation: 3,
            token: "token-123".to_string(),
        };

        let session = seat_session_json(99, &access);

        assert_eq!(session["seat"], 2);
        assert_eq!(session["generation"], 3);
        assert_eq!(session["token"], "token-123");
        assert_eq!(session["status_path"], "/room/99");
        assert_eq!(session["enter_path"], "/enter/99");
        assert_eq!(session["query"]["seat"], 2);
        assert_eq!(session["query"]["generation"], 3);
        assert_eq!(session["query"]["token"], "token-123");
    }

    #[test]
    fn join_options_json_tracks_open_seats() {
        let options = join_options_json(77, &[1, 3]);

        assert_eq!(options.len(), 2);
        assert_eq!(options[0]["seat"], 1);
        assert_eq!(options[0]["join_path"], "/join/77");
        assert_eq!(options[0]["body"]["target_seat"], 1);
        assert_eq!(options[1]["seat"], 3);
        assert_eq!(options[1]["body"]["target_seat"], 3);
    }

    #[test]
    fn resume_json_wraps_seat_sessions() {
        let access = SeatAccess {
            seat: 2,
            generation: 3,
            token: "token-123".to_string(),
        };

        let resume = resume_json(99, &[access]);

        assert_eq!(resume["room_id"], 99);
        assert_eq!(resume["seat_sessions"][0]["seat"], 2);
        assert_eq!(resume["seat_sessions"][0]["generation"], 3);
    }

    #[test]
    fn invitation_json_wraps_join_options() {
        let invitation = invitation_json(77, "room-deadbeef", &[1, 3]);

        assert_eq!(invitation["room_id"], 77);
        assert_eq!(invitation["invitation_id"], "room-deadbeef");
        assert_eq!(invitation["invite_path"], "/invite/room-deadbeef");
        assert_eq!(invitation["lobby_path"], "/lobby/77");
        assert_eq!(invitation["join_options"][0]["seat"], 1);
        assert_eq!(invitation["join_options"][1]["seat"], 3);
    }

    #[test]
    fn invite_not_found_returns_not_found() {
        let response = actix_web::rt::System::new().block_on(async {
            let casino = web::Data::new(Casino::default());
            invite(casino, web::Path::from("room-missing".to_string()))
                .await
                .respond_to(&actix_web::test::TestRequest::default().to_http_request())
        });

        assert_eq!(response.status(), actix_web::http::StatusCode::NOT_FOUND);
    }

    #[test]
    fn lobby_not_found_returns_not_found() {
        let response = actix_web::rt::System::new().block_on(async {
            let casino = web::Data::new(Casino::default());
            lobby(casino, web::Path::from(999_u64))
                .await
                .respond_to(&actix_web::test::TestRequest::default().to_http_request())
        });

        assert_eq!(response.status(), actix_web::http::StatusCode::NOT_FOUND);
    }

    #[test]
    fn join_seat_error_response_uses_expected_status_codes() {
        assert_eq!(
            join_seat_error_response(JoinSeatError::RoomNotFound).status(),
            actix_web::http::StatusCode::NOT_FOUND
        );
        assert_eq!(
            join_seat_error_response(JoinSeatError::SeatUnavailable {
                seat: 2,
                kind: SeatKind::Human,
            })
            .status(),
            actix_web::http::StatusCode::CONFLICT
        );
        assert_eq!(
            join_seat_error_response(JoinSeatError::SeatOutOfRange { seat: 7 }).status(),
            actix_web::http::StatusCode::BAD_REQUEST
        );
    }
}
