use super::*;
use crate::cards::*;
use crate::gameplay::*;
use actix_cors::Cors;
use actix_web::App;
use actix_web::HttpResponse;
use actix_web::HttpServer;
use actix_web::Responder;
use actix_web::middleware::Logger;
use actix_web::web;

// WRT_NEIGHBOR_ABS = "/nbr-wrt-abs", // TODO implement in server.rs

pub struct Server;

impl Server {
    pub async fn run() -> Result<(), std::io::Error> {
        let api = web::Data::new(API::from_env().await.map_err(std::io::Error::other)?);
        log::info!("starting HTTP server");
        HttpServer::new(move || {
            App::new()
                .wrap(Logger::new("%r %s %Ts"))
                .wrap(
                    Cors::default()
                        .allow_any_origin()
                        .allow_any_method()
                        .allow_any_header(),
                )
                .app_data(api.clone())
                .route("/health", web::get().to(health))
                .route("/replace-obs", web::post().to(replace_obs))
                .route("/nbr-any-abs", web::post().to(nbr_any_wrt_abs))
                .route("/nbr-obs-abs", web::post().to(nbr_obs_wrt_abs))
                .route("/nbr-abs-abs", web::post().to(nbr_abs_wrt_abs))
                .route("/nbr-kfn-abs", web::post().to(kfn_wrt_abs))
                .route("/nbr-knn-abs", web::post().to(knn_wrt_abs))
                .route("/nbr-kgn-abs", web::post().to(kgn_wrt_abs))
                .route("/exp-wrt-str", web::post().to(exp_wrt_str))
                .route("/exp-wrt-abs", web::post().to(exp_wrt_abs))
                .route("/exp-wrt-obs", web::post().to(exp_wrt_obs))
                .route("/hst-wrt-abs", web::post().to(hst_wrt_abs))
                .route("/hst-wrt-obs", web::post().to(hst_wrt_obs))
                .route("/blueprint", web::post().to(blueprint))
        })
        .workers(6)
        .bind(std::env::var("BIND_ADDR").expect("BIND_ADDR must be set"))?
        .run()
        .await
    }
}

// Route handlers

async fn health() -> impl Responder {
    HttpResponse::Ok().body("ok")
}

async fn replace_obs(api: web::Data<API>, req: web::Json<ReplaceObs>) -> impl Responder {
    match api.parse_observation_target(req.obs.as_str()) {
        Err(err) => HttpResponse::BadRequest().body(err.to_string()),
        Ok((obs, seat_position)) => match api.replace_obs(obs, seat_position).await {
            Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
            Ok(new) => HttpResponse::Ok().json(new.to_string()),
        },
    }
}

async fn exp_wrt_str(api: web::Data<API>, req: web::Json<SetStreets>) -> impl Responder {
    let street = Street::try_from(req.street.as_str());
    match street {
        Err(_) => HttpResponse::BadRequest().body("invalid street format"),
        Ok(street) => match api.exp_wrt_str(street, req.seat_position).await {
            Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
            Ok(row) => HttpResponse::Ok().json(row),
        },
    }
}
async fn exp_wrt_abs(api: web::Data<API>, req: web::Json<ReplaceAbs>) -> impl Responder {
    let wrt = Abstraction::try_from(req.wrt.as_str());
    match wrt {
        Err(_) => HttpResponse::BadRequest().body("invalid abstraction format"),
        Ok(abs) => match api.exp_wrt_abs(abs).await {
            Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
            Ok(row) => HttpResponse::Ok().json(row),
        },
    }
}
async fn exp_wrt_obs(api: web::Data<API>, req: web::Json<RowWrtObs>) -> impl Responder {
    match api.parse_observation_target(req.obs.as_str()) {
        Err(err) => HttpResponse::BadRequest().body(err.to_string()),
        Ok((obs, seat_position)) => match api.exp_wrt_obs(obs, seat_position).await {
            Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
            Ok(row) => HttpResponse::Ok().json(row),
        },
    }
}

async fn nbr_any_wrt_abs(api: web::Data<API>, req: web::Json<ReplaceAbs>) -> impl Responder {
    let wrt = Abstraction::try_from(req.wrt.as_str());
    match wrt {
        Err(_) => HttpResponse::BadRequest().body("invalid abstraction format"),
        Ok(abs) => match api.nbr_any_wrt_abs(abs).await {
            Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
            Ok(row) => HttpResponse::Ok().json(row),
        },
    }
}
async fn nbr_abs_wrt_abs(api: web::Data<API>, req: web::Json<ReplaceOne>) -> impl Responder {
    let wrt = Abstraction::try_from(req.wrt.as_str());
    let abs = Abstraction::try_from(req.abs.as_str());
    match (wrt, abs) {
        (Err(_), _) => HttpResponse::BadRequest().body("invalid abstraction format"),
        (_, Err(_)) => HttpResponse::BadRequest().body("invalid abstraction format"),
        (Ok(wrt), Ok(abs)) => match api.nbr_abs_wrt_abs(wrt, abs).await {
            Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
            Ok(row) => HttpResponse::Ok().json(row),
        },
    }
}
async fn nbr_obs_wrt_abs(api: web::Data<API>, req: web::Json<ReplaceRow>) -> impl Responder {
    let wrt = Abstraction::try_from(req.wrt.as_str());
    let obs = api.parse_observation_target(req.obs.as_str());
    match (wrt, obs) {
        (Err(_), _) => HttpResponse::BadRequest().body("invalid abstraction format"),
        (_, Err(err)) => HttpResponse::BadRequest().body(err.to_string()),
        (Ok(abs), Ok((obs, seat_position))) => {
            match api.nbr_obs_wrt_abs(abs, obs, seat_position).await {
                Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
                Ok(rows) => HttpResponse::Ok().json(rows),
            }
        }
    }
}

async fn kfn_wrt_abs(api: web::Data<API>, req: web::Json<ReplaceAbs>) -> impl Responder {
    let wrt = Abstraction::try_from(req.wrt.as_str());
    match wrt {
        Err(_) => HttpResponse::BadRequest().body("invalid abstraction format"),
        Ok(abs) => match api.kfn_wrt_abs(abs).await {
            Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
            Ok(rows) => HttpResponse::Ok().json(rows),
        },
    }
}
async fn knn_wrt_abs(api: web::Data<API>, req: web::Json<ReplaceAbs>) -> impl Responder {
    let wrt = Abstraction::try_from(req.wrt.as_str());
    match wrt {
        Err(_) => HttpResponse::BadRequest().body("invalid abstraction format"),
        Ok(abs) => match api.knn_wrt_abs(abs).await {
            Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
            Ok(rows) => HttpResponse::Ok().json(rows),
        },
    }
}
async fn kgn_wrt_abs(api: web::Data<API>, req: web::Json<ReplaceAll>) -> impl Responder {
    let wrt = Abstraction::try_from(req.wrt.as_str());
    match wrt {
        Err(_) => HttpResponse::BadRequest().body("invalid abstraction format"),
        Ok(wrt) => {
            let obs = req
                .neighbors
                .iter()
                .map(|string| api.parse_observation_target(string))
                .filter_map(|result| result.ok())
                .filter(|(obs, _)| obs.street() == wrt.street())
                .chain((0..).map(|_| (Observation::from(wrt.street()), 0)))
                .take(5)
                .collect::<Vec<_>>();
            match api.kgn_wrt_abs(wrt, obs).await {
                Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
                Ok(rows) => HttpResponse::Ok().json(rows),
            }
        }
    }
}

async fn hst_wrt_abs(api: web::Data<API>, req: web::Json<AbsHist>) -> impl Responder {
    let abs = Abstraction::try_from(req.abs.as_str());
    match abs {
        Err(_) => HttpResponse::BadRequest().body("invalid abstraction format"),
        Ok(abs) => match api.hst_wrt_abs(abs).await {
            Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
            Ok(rows) => HttpResponse::Ok().json(rows),
        },
    }
}

async fn hst_wrt_obs(api: web::Data<API>, req: web::Json<ObsHist>) -> impl Responder {
    match api.parse_observation_target(req.obs.as_str()) {
        Err(err) => HttpResponse::BadRequest().body(err.to_string()),
        Ok((obs, seat_position)) => match api.hst_wrt_obs(obs, seat_position).await {
            Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
            Ok(rows) => HttpResponse::Ok().json(rows),
        },
    }
}

async fn blueprint(api: web::Data<API>, req: web::Json<GetPolicy>) -> impl Responder {
    match build_policy_recall(&req) {
        Err(err) => HttpResponse::BadRequest().body(err),
        Ok(recall) => match api.policy(recall).await {
            Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
            Ok(Some(strategy)) => HttpResponse::Ok().json(strategy),
            Ok(None) => HttpResponse::Ok().json(serde_json::Value::Null),
        },
    }
}

fn parse_policy_table_config(req: &GetPolicy) -> Result<Option<TableConfig>, String> {
    let any_set = req.seat_count.is_some()
        || req.small_blind.is_some()
        || req.big_blind.is_some()
        || req.ante.is_some()
        || req.starting_stack.is_some();
    if !any_set {
        return Ok(None);
    }

    let seat_count = req.seat_count.ok_or_else(|| {
        "seat_count is required when providing blueprint table config".to_string()
    })?;
    let small_blind = req.small_blind.ok_or_else(|| {
        "small_blind is required when providing blueprint table config".to_string()
    })?;
    let big_blind = req
        .big_blind
        .ok_or_else(|| "big_blind is required when providing blueprint table config".to_string())?;
    let ante = req.ante.unwrap_or(0);
    let starting_stack = req.starting_stack.ok_or_else(|| {
        "starting_stack is required when providing blueprint table config".to_string()
    })?;

    let config = TableConfig {
        seat_count,
        small_blind,
        big_blind,
        ante,
        starting_stack,
    };
    config
        .validate()
        .map_err(|err| format!("invalid table config: {}", err))?;
    Ok(Some(config))
}

fn build_policy_recall(req: &GetPolicy) -> Result<Recall, String> {
    let hero = Turn::try_from(req.turn.as_str()).map_err(|_| "invalid player turn".to_string())?;
    let seen = Observation::try_from(req.seen.as_str())
        .map_err(|_| "invalid observation format".to_string())?;
    let path = req
        .past
        .iter()
        .map(|string| string.as_str())
        .map(Action::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "invalid action history".to_string())?;
    let config = parse_policy_table_config(req)?;
    Ok(match config {
        Some(config) => Recall::from_actions_with_config(hero, seen, path, config),
        None => Recall::from((hero, seen, path)),
    })
}

#[cfg(test)]
mod tests {
    use super::{build_policy_recall, parse_policy_table_config};
    use crate::analysis::GetPolicy;
    use crate::gameplay::Action;

    #[test]
    fn parse_policy_table_config_returns_none_when_omitted() {
        let req = GetPolicy {
            turn: "P0".to_string(),
            seen: "2c 3c".to_string(),
            past: vec![],
            seat_count: None,
            small_blind: None,
            big_blind: None,
            ante: None,
            starting_stack: None,
        };
        assert!(parse_policy_table_config(&req).unwrap().is_none());
    }

    #[test]
    fn parse_policy_table_config_builds_valid_multiway_config() {
        let req = GetPolicy {
            turn: "P1".to_string(),
            seen: "2c 3c".to_string(),
            past: vec![],
            seat_count: Some(6),
            small_blind: Some(1),
            big_blind: Some(2),
            ante: Some(1),
            starting_stack: Some(200),
        };
        let config = parse_policy_table_config(&req).unwrap().unwrap();
        assert_eq!(config.seat_count, 6);
        assert_eq!(config.small_blind, 1);
        assert_eq!(config.big_blind, 2);
        assert_eq!(config.ante, 1);
        assert_eq!(config.starting_stack, 200);
    }

    #[test]
    fn parse_policy_table_config_ignores_street_request_seat_position() {
        let req = GetPolicy {
            turn: "P0".to_string(),
            seen: "2c 3c".to_string(),
            past: vec![],
            seat_count: None,
            small_blind: None,
            big_blind: None,
            ante: None,
            starting_stack: None,
        };
        assert!(parse_policy_table_config(&req).unwrap().is_none());
    }

    #[test]
    fn build_policy_recall_uses_configured_posting_prefix() {
        let req = GetPolicy {
            turn: "P0".to_string(),
            seen: "2c 3c".to_string(),
            past: vec!["CALL 5".to_string()],
            seat_count: Some(3),
            small_blind: Some(2),
            big_blind: Some(5),
            ante: Some(1),
            starting_stack: Some(200),
        };
        let recall = build_policy_recall(&req).unwrap();
        assert_eq!(recall.config().unwrap().seat_count, 3);
        assert_eq!(recall.actions().len(), 6);
        assert_eq!(recall.actions()[0], Action::Blind(1));
        assert_eq!(recall.actions()[3], Action::Blind(2));
        assert_eq!(recall.actions()[4], Action::Blind(5));
        assert_eq!(recall.actions()[5], Action::Call(5));
    }
}
