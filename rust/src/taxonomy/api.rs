use crate::parse::parse_file;
use crate::parse::{lookup::TaxonInfo, nodes::Nodes};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    serve, Json, Router,
};
use blart::TreeMap;
use std::ffi::CString;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tokio::net::TcpListener;

use crate::error::Error;

pub struct TaxonomyService {
    pub nodes: Nodes,
    pub id_map: TreeMap<CString, Vec<TaxonInfo>>,
}

impl TaxonomyService {
    pub fn new(nodes: Nodes, id_map: TreeMap<CString, Vec<TaxonInfo>>) -> Self {
        Self { nodes, id_map }
    }
}

#[derive(serde::Deserialize)]
pub struct ValidateRequest {
    pub file_path: String, // Only file_path allowed
}

#[derive(serde::Serialize)]
pub struct ValidateResponse {
    pub valid: bool,
    pub message: String,
}

async fn validate_handler(
    State(service): State<Arc<RwLock<TaxonomyService>>>,
    Json(req): Json<ValidateRequest>,
) -> Json<ValidateResponse> {
    let service = service.read().unwrap();
    let result = parse_file(
        std::path::PathBuf::from(&req.file_path),
        &service.id_map,
        true,
        false,
        None,
    );
    match result {
        Ok(_) => Json(ValidateResponse {
            valid: true,
            message: "OK".to_string(),
        }),
        Err(e) => Json(ValidateResponse {
            valid: false,
            message: format!("{:?}", e),
        }),
    }
}

#[derive(serde::Serialize)]
pub struct NodeResponse {
    pub found: bool,
    pub node: Option<crate::parse::nodes::Node>,
}

async fn node_handler(
    State(service): State<Arc<RwLock<TaxonomyService>>>,
    Path(id): Path<String>,
) -> Json<NodeResponse> {
    let service = service.read().unwrap();
    if let Some(node) = service.nodes.nodes.get(&id) {
        Json(NodeResponse {
            found: true,
            node: Some(node.clone()),
        })
    } else {
        Json(NodeResponse {
            found: false,
            node: None,
        })
    }
}

#[derive(serde::Serialize)]
pub struct LookupResponse {
    pub name: String,
    pub found: bool,
    pub ids: Vec<String>,
}

#[derive(serde::Deserialize)]
pub struct LookupListRequest {
    pub names: Vec<String>,
}

async fn lookup_handler(
    State(service): State<Arc<RwLock<TaxonomyService>>>,
    Path(name): Path<String>,
) -> Json<LookupResponse> {
    let service = service.read().unwrap();
    let key = CString::new(name.clone()).unwrap_or_default();
    let ids = service
        .id_map
        .get(&key)
        .map(|v| v.iter().map(|ti| ti.tax_id.clone()).collect())
        .unwrap_or_else(Vec::new);
    Json(LookupResponse {
        name,
        found: !ids.is_empty(),
        ids,
    })
}

async fn lookup_list_handler(
    State(service): State<Arc<RwLock<TaxonomyService>>>,
    Json(req): Json<LookupListRequest>,
) -> Json<Vec<LookupResponse>> {
    let service = service.read().unwrap();
    let mut results = Vec::new();
    for name in req.names {
        let key = CString::new(name.clone()).unwrap_or_default();
        let ids = service
            .id_map
            .get(&key)
            .map(|v| v.iter().map(|ti| ti.tax_id.clone()).collect())
            .unwrap_or_else(Vec::new);
        results.push(LookupResponse {
            name,
            found: !ids.is_empty(),
            ids,
        });
    }
    Json(results)
}

pub async fn run_api_server(service: Arc<RwLock<TaxonomyService>>, port: u16) -> Result<(), Error> {
    let app = Router::new()
        .route("/validate", post(validate_handler))
        .route("/node/{id}", get(node_handler))
        .route("/lookup/{name}", get(lookup_handler))
        .route("/lookup", post(lookup_list_handler))
        .with_state(service);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("Starting API server on {}", addr);

    let listener = TcpListener::bind(addr).await?;
    serve(listener, app.into_make_service()).await?;

    Ok(())
}
