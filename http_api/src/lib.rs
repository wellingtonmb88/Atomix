use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};
use solana_pubkey::Pubkey;
use std::net::SocketAddr;
use std::str::FromStr;

use greedy_scheduler::api::SchedulerApi;

/// Helper function to decode tx hash from hex, base58, or base64
fn decode_tx_hash(hash_str: &str) -> Result<Vec<u8>, String> {
    // Try base58 decoding (Solana format for signatures/pubkeys)
    if let Ok(bytes) = bs58::decode(hash_str).into_vec() {
        return Ok(bytes);
    }

    // Try hex decoding first (most common for tx hashes)
    if let Ok(bytes) = hex::decode(hash_str) {
        return Ok(bytes);
    }

    // Try base64 decoding
    if let Ok(bytes) = general_purpose::STANDARD.decode(hash_str) {
        return Ok(bytes);
    }

    Err(format!("Failed to decode tx hash '{}'. Supported formats: hex, base58, base64", hash_str))
}

/// Helper function to encode Vec<u8> to hex string
fn encode_tx_hash(bytes: &[u8]) -> String {
    use solana_sdk::hash::Hash;
    let array: [u8; 32] = bytes.try_into().expect("slice with incorrect length");
    Hash::new_from_array(array).to_string()
}

/// Shared state for HTTP handlers
#[derive(Clone)]
struct AppState {
    api: SchedulerApi,
}

/// Request to add a signer to priority list
#[derive(Debug, Deserialize)]
pub struct AddSignerRequest {
    /// Pubkey as string
    pub pubkey: String,
    /// Optional quota (if None, unlimited)
    pub quota: Option<u64>,
}

/// Request to add a program to priority list
#[derive(Debug, Deserialize)]
pub struct AddProgramRequest {
    /// Pubkey as string
    pub pubkey: String,
    /// Optional quota (if None, unlimited)
    pub quota: Option<u64>,
}

/// Request to set priority multiplier
#[derive(Debug, Deserialize)]
pub struct SetMultiplierRequest {
    /// Priority multiplier value
    pub multiplier: u64,
}

/// Request to increment quota
#[derive(Debug, Deserialize)]
pub struct IncrementQuotaRequest {
    /// Amount to increment
    pub amount: u64,
}

/// Request to add bundle signer/program
#[derive(Debug, Deserialize)]
pub struct AddBundleRequest {
    /// Pubkey as string
    pub pubkey: String,
    /// Tip amount
    pub tip: u64,
    /// Transaction hashes (as base64 or hex strings)
    pub tx_hashes: Vec<String>,
}

/// Request to update bundle tip
#[derive(Debug, Deserialize)]
pub struct UpdateBundleTipRequest {
    /// New tip amount
    pub tip: u64,
}

/// Request to add tx hashes to bundle
#[derive(Debug, Deserialize)]
pub struct AddTxHashesRequest {
    /// Transaction hashes to add (as base64 or hex strings)
    pub tx_hashes: Vec<String>,
}

/// Response for bundle entry
#[derive(Debug, Serialize)]
pub struct BundleEntryResponse {
    /// Pubkey as string
    pub pubkey: String,
    /// Tip amount
    pub tip: u64,
    /// Transaction hashes (as hex strings)
    pub tx_hashes: Vec<String>,
    /// Number of transactions in bundle
    pub tx_count: usize,
}

/// Response for signer statistics
#[derive(Debug, Serialize)]
pub struct SignerStatsResponse {
    /// Pubkey as string
    pub pubkey: String,
    /// Total transactions executed
    pub total_executed: u64,
    /// Successful transactions
    pub successful: u64,
    /// Success rate (0.0 to 1.0)
    pub success_rate: f64,
    /// Remaining quota (None if not in priority list)
    pub quota_remaining: Option<u64>,
    /// Initial quota (None if not in priority list)
    pub quota_initial: Option<u64>,
    /// Quota usage percentage (0.0 to 1.0, None if not in priority list)
    pub quota_usage_pct: Option<f64>,
}

/// Response for program statistics
#[derive(Debug, Serialize)]
pub struct ProgramStatsResponse {
    /// Pubkey as string
    pub pubkey: String,
    /// Total transactions executed
    pub total_executed: u64,
    /// Successful transactions
    pub successful: u64,
    /// Success rate (0.0 to 1.0)
    pub success_rate: f64,
    /// Remaining quota (None if not in priority list)
    pub quota_remaining: Option<u64>,
    /// Initial quota (None if not in priority list)
    pub quota_initial: Option<u64>,
    /// Quota usage percentage (0.0 to 1.0, None if not in priority list)
    pub quota_usage_pct: Option<f64>,
}

/// Response for quota information
#[derive(Debug, Serialize)]
pub struct QuotaResponse {
    /// Pubkey as string
    pub pubkey: String,
    /// Remaining quota
    pub remaining_quota: u64,
    /// Initial quota
    pub initial_quota: u64,
    /// Usage percentage (0.0 to 1.0)
    pub usage_percentage: f64,
}

/// Generic success response
#[derive(Debug, Serialize)]
pub struct SuccessResponse {
    pub message: String,
}

/// Generic error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// Add a signer to priority list
async fn add_signer(
    State(state): State<AppState>,
    Json(req): Json<AddSignerRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pubkey = Pubkey::from_str(&req.pubkey).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid pubkey: {}", e) }))
    })?;

    match req.quota {
        Some(quota) => state.api.add_priority_signer(pubkey, quota),
        None => state.api.add_priority_signer_unlimited(pubkey),
    }
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to add signer: {}", e) }),
        )
    })?;

    Ok(Json(SuccessResponse { message: format!("Signer {} added successfully", req.pubkey) }))
}

/// Remove a signer from priority list
async fn remove_signer(
    State(state): State<AppState>,
    Path(pubkey_str): Path<String>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pubkey = Pubkey::from_str(&pubkey_str).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid pubkey: {}", e) }))
    })?;

    state.api.remove_priority_signer(pubkey).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to remove signer: {}", e) }),
        )
    })?;

    Ok(Json(SuccessResponse { message: format!("Signer {} removed successfully", pubkey_str) }))
}

/// Get signer statistics
async fn get_signer_stats(
    State(state): State<AppState>,
    Path(pubkey_str): Path<String>,
) -> Result<Json<SignerStatsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pubkey = Pubkey::from_str(&pubkey_str).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid pubkey: {}", e) }))
    })?;

    let stats = state.api.get_signer_stats(pubkey).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to get signer stats: {}", e) }),
        )
    })?;

    let quota = state.api.get_signer_quota(pubkey).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to get signer quota: {}", e) }),
        )
    })?;

    Ok(Json(SignerStatsResponse {
        pubkey: pubkey_str,
        total_executed: stats.map(|s| s.total_executed).unwrap_or(0),
        successful: stats.map(|s| s.successful).unwrap_or(0),
        success_rate: stats.map(|s| s.success_rate()).unwrap_or(0.0),
        quota_remaining: quota.map(|q| q.remaining_quota),
        quota_initial: quota.map(|q| q.initial_quota),
        quota_usage_pct: quota.map(|q| q.usage_percentage()),
    }))
}

/// Get signer quota
async fn get_signer_quota(
    State(state): State<AppState>,
    Path(pubkey_str): Path<String>,
) -> Result<Json<QuotaResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pubkey = Pubkey::from_str(&pubkey_str).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid pubkey: {}", e) }))
    })?;

    let quota = state
        .api
        .get_signer_quota(pubkey)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: format!("Failed to get signer quota: {}", e) }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Signer {} not found in priority list", pubkey_str),
                }),
            )
        })?;

    Ok(Json(QuotaResponse {
        pubkey: pubkey_str,
        remaining_quota: quota.remaining_quota,
        initial_quota: quota.initial_quota,
        usage_percentage: quota.usage_percentage(),
    }))
}

/// Add a program to priority list
async fn add_program(
    State(state): State<AppState>,
    Json(req): Json<AddProgramRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pubkey = Pubkey::from_str(&req.pubkey).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid pubkey: {}", e) }))
    })?;

    match req.quota {
        Some(quota) => state.api.add_priority_program(pubkey, quota),
        None => state.api.add_priority_program_unlimited(pubkey),
    }
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to add program: {}", e) }),
        )
    })?;

    Ok(Json(SuccessResponse { message: format!("Program {} added successfully", req.pubkey) }))
}

/// Remove a program from priority list
async fn remove_program(
    State(state): State<AppState>,
    Path(pubkey_str): Path<String>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pubkey = Pubkey::from_str(&pubkey_str).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid pubkey: {}", e) }))
    })?;

    state.api.remove_priority_program(pubkey).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to remove program: {}", e) }),
        )
    })?;

    Ok(Json(SuccessResponse { message: format!("Program {} removed successfully", pubkey_str) }))
}

/// Get program statistics
async fn get_program_stats(
    State(state): State<AppState>,
    Path(pubkey_str): Path<String>,
) -> Result<Json<ProgramStatsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pubkey = Pubkey::from_str(&pubkey_str).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid pubkey: {}", e) }))
    })?;

    let stats = state.api.get_program_stats(pubkey).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to get program stats: {}", e) }),
        )
    })?;

    let quota = state.api.get_program_quota(pubkey).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to get program quota: {}", e) }),
        )
    })?;

    Ok(Json(ProgramStatsResponse {
        pubkey: pubkey_str,
        total_executed: stats.map(|s| s.total_executed).unwrap_or(0),
        successful: stats.map(|s| s.successful).unwrap_or(0),
        success_rate: stats.map(|s| s.success_rate()).unwrap_or(0.0),
        quota_remaining: quota.map(|q| q.remaining_quota),
        quota_initial: quota.map(|q| q.initial_quota),
        quota_usage_pct: quota.map(|q| q.usage_percentage()),
    }))
}

/// Get program quota
async fn get_program_quota(
    State(state): State<AppState>,
    Path(pubkey_str): Path<String>,
) -> Result<Json<QuotaResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pubkey = Pubkey::from_str(&pubkey_str).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid pubkey: {}", e) }))
    })?;

    let quota = state
        .api
        .get_program_quota(pubkey)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: format!("Failed to get program quota: {}", e) }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Program {} not found in priority list", pubkey_str),
                }),
            )
        })?;

    Ok(Json(QuotaResponse {
        pubkey: pubkey_str,
        remaining_quota: quota.remaining_quota,
        initial_quota: quota.initial_quota,
        usage_percentage: quota.usage_percentage(),
    }))
}

/// Set priority multiplier
async fn set_multiplier(
    State(state): State<AppState>,
    Json(req): Json<SetMultiplierRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    state
        .api
        .set_priority_multiplier(req.multiplier)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: format!("Failed to set multiplier: {}", e) }),
            )
        })?;

    Ok(Json(SuccessResponse { message: format!("Priority multiplier set to {}", req.multiplier) }))
}

/// Clear all priority lists
async fn clear_lists(
    State(state): State<AppState>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    state.api.clear_priority_lists().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to clear priority lists: {}", e) }),
        )
    })?;

    Ok(Json(SuccessResponse { message: "All priority lists cleared successfully".to_string() }))
}

/// Clear statistics
async fn clear_statistics(
    State(state): State<AppState>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    state.api.clear_stats().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to clear statistics: {}", e) }),
        )
    })?;

    Ok(Json(SuccessResponse { message: "Statistics cleared successfully".to_string() }))
}

/// Increment signer quota
async fn increment_signer_quota(
    State(state): State<AppState>,
    Path(pubkey_str): Path<String>,
    Json(req): Json<IncrementQuotaRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pubkey = Pubkey::from_str(&pubkey_str).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid pubkey: {}", e) }))
    })?;

    state
        .api
        .increment_signer_quota(pubkey, req.amount)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: format!("Failed to increment signer quota: {}", e) }),
            )
        })?;

    Ok(Json(SuccessResponse {
        message: format!("Signer {} quota incremented by {}", pubkey_str, req.amount),
    }))
}

/// Increment program quota
async fn increment_program_quota(
    State(state): State<AppState>,
    Path(pubkey_str): Path<String>,
    Json(req): Json<IncrementQuotaRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pubkey = Pubkey::from_str(&pubkey_str).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid pubkey: {}", e) }))
    })?;

    state
        .api
        .increment_program_quota(pubkey, req.amount)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: format!("Failed to increment program quota: {}", e) }),
            )
        })?;

    Ok(Json(SuccessResponse {
        message: format!("Program {} quota incremented by {}", pubkey_str, req.amount),
    }))
}

/// Health check endpoint
async fn health() -> Json<SuccessResponse> {
    Json(SuccessResponse { message: "Scheduler API is running".to_string() })
}

/// Response for all metrics endpoint
#[derive(Debug, Serialize)]
pub struct AllMetricsResponse {
    pub signers: std::collections::HashMap<String, ExecutionStatsResponse>,
    pub programs: std::collections::HashMap<String, ExecutionStatsResponse>,
    pub total_signers: usize,
    pub total_programs: usize,
}

/// Response for execution statistics
#[derive(Debug, Serialize)]
pub struct ExecutionStatsResponse {
    pub total_executed: u64,
    pub successful: u64,
    pub success_rate: f64,
}

/// Get all metrics (signers + programs)
async fn get_all_metrics(
    State(state): State<AppState>,
) -> Result<Json<AllMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let signer_stats = state.api.get_all_signer_stats().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to get signer stats: {}", e) }),
        )
    })?;

    let program_stats = state.api.get_all_program_stats().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to get program stats: {}", e) }),
        )
    })?;

    let signers = signer_stats
        .iter()
        .map(|(pubkey, stats)| {
            (
                pubkey.to_string(),
                ExecutionStatsResponse {
                    total_executed: stats.total_executed,
                    successful: stats.successful,
                    success_rate: stats.success_rate(),
                },
            )
        })
        .collect();

    let programs = program_stats
        .iter()
        .map(|(pubkey, stats)| {
            (
                pubkey.to_string(),
                ExecutionStatsResponse {
                    total_executed: stats.total_executed,
                    successful: stats.successful,
                    success_rate: stats.success_rate(),
                },
            )
        })
        .collect();

    Ok(Json(AllMetricsResponse {
        signers,
        programs,
        total_signers: signer_stats.len(),
        total_programs: program_stats.len(),
    }))
}

/// Get all signer stats
async fn get_all_signer_stats(
    State(state): State<AppState>,
) -> Result<
    Json<std::collections::HashMap<String, ExecutionStatsResponse>>,
    (StatusCode, Json<ErrorResponse>),
> {
    let signer_stats = state.api.get_all_signer_stats().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to get signer stats: {}", e) }),
        )
    })?;

    let stats = signer_stats
        .iter()
        .map(|(pubkey, stats)| {
            (
                pubkey.to_string(),
                ExecutionStatsResponse {
                    total_executed: stats.total_executed,
                    successful: stats.successful,
                    success_rate: stats.success_rate(),
                },
            )
        })
        .collect();

    Ok(Json(stats))
}

/// Get all program stats
async fn get_all_program_stats(
    State(state): State<AppState>,
) -> Result<
    Json<std::collections::HashMap<String, ExecutionStatsResponse>>,
    (StatusCode, Json<ErrorResponse>),
> {
    let program_stats = state.api.get_all_program_stats().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to get program stats: {}", e) }),
        )
    })?;

    let stats = program_stats
        .iter()
        .map(|(pubkey, stats)| {
            (
                pubkey.to_string(),
                ExecutionStatsResponse {
                    total_executed: stats.total_executed,
                    successful: stats.successful,
                    success_rate: stats.success_rate(),
                },
            )
        })
        .collect();

    Ok(Json(stats))
}

/// Get performance metrics
async fn get_performance_metrics(
    State(state): State<AppState>,
) -> Result<Json<greedy_scheduler::PerformanceSnapshot>, (StatusCode, Json<ErrorResponse>)> {
    let metrics = state.api.get_performance_metrics().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to get performance metrics: {}", e) }),
        )
    })?;

    Ok(Json(metrics))
}

/// Reset performance metrics
async fn reset_performance_metrics(
    State(state): State<AppState>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    state.api.reset_performance_metrics().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to reset performance metrics: {}", e) }),
        )
    })?;

    Ok(Json(SuccessResponse { message: "Performance metrics reset successfully".to_string() }))
}

// === Bundle Signer Handlers ===

/// Add bundle signer
async fn add_bundle_signer(
    State(state): State<AppState>,
    Json(req): Json<AddBundleRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pubkey = Pubkey::from_str(&req.pubkey).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid pubkey: {}", e) }))
    })?;

    // Decode tx hashes
    let tx_hashes: Result<Vec<Vec<u8>>, String> =
        req.tx_hashes.iter().map(|h| decode_tx_hash(h)).collect();

    let tx_hashes =
        tx_hashes.map_err(|e| (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })))?;

    state
        .api
        .add_bundle_signer(pubkey, req.tip, tx_hashes)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: format!("Failed to add bundle signer: {}", e) }),
            )
        })?;

    Ok(Json(SuccessResponse {
        message: format!("Bundle signer {} added successfully", req.pubkey),
    }))
}

/// Remove bundle signer
async fn remove_bundle_signer(
    State(state): State<AppState>,
    Path(pubkey_str): Path<String>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pubkey = Pubkey::from_str(&pubkey_str).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid pubkey: {}", e) }))
    })?;

    state.api.remove_bundle_signer(pubkey).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to remove bundle signer: {}", e) }),
        )
    })?;

    Ok(Json(SuccessResponse {
        message: format!("Bundle signer {} removed successfully", pubkey_str),
    }))
}

/// Get bundle signer
async fn get_bundle_signer(
    State(state): State<AppState>,
    Path(pubkey_str): Path<String>,
) -> Result<Json<BundleEntryResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pubkey = Pubkey::from_str(&pubkey_str).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid pubkey: {}", e) }))
    })?;

    let bundle = state
        .api
        .get_bundle_signer(pubkey)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: format!("Failed to get bundle signer: {}", e) }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse { error: format!("Bundle signer {} not found", pubkey_str) }),
            )
        })?;

    Ok(Json(BundleEntryResponse {
        pubkey: pubkey_str,
        tip: bundle.tip,
        tx_count: bundle.tx_hashes.len(),
        tx_hashes: bundle.tx_hashes.iter().map(|h| encode_tx_hash(h)).collect(),
    }))
}

/// Update bundle signer tip
async fn update_bundle_signer_tip(
    State(state): State<AppState>,
    Path(pubkey_str): Path<String>,
    Json(req): Json<UpdateBundleTipRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pubkey = Pubkey::from_str(&pubkey_str).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid pubkey: {}", e) }))
    })?;

    state
        .api
        .update_bundle_signer_tip(pubkey, req.tip)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: format!("Failed to update bundle signer tip: {}", e) }),
            )
        })?;

    Ok(Json(SuccessResponse {
        message: format!("Bundle signer {} tip updated to {}", pubkey_str, req.tip),
    }))
}

/// Add tx hashes to bundle signer
async fn add_bundle_signer_tx_hashes(
    State(state): State<AppState>,
    Path(pubkey_str): Path<String>,
    Json(req): Json<AddTxHashesRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pubkey = Pubkey::from_str(&pubkey_str).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid pubkey: {}", e) }))
    })?;

    // Decode tx hashes
    let tx_hashes: Result<Vec<Vec<u8>>, String> =
        req.tx_hashes.iter().map(|h| decode_tx_hash(h)).collect();

    let tx_hashes =
        tx_hashes.map_err(|e| (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })))?;

    state
        .api
        .add_bundle_signer_tx_hashes(pubkey, tx_hashes.clone())
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to add tx hashes to bundle signer: {}", e),
                }),
            )
        })?;

    Ok(Json(SuccessResponse {
        message: format!("Added {} tx hashes to bundle signer {}", tx_hashes.len(), pubkey_str),
    }))
}

/// Get all bundle signers
async fn get_all_bundle_signers(
    State(state): State<AppState>,
) -> Result<
    Json<std::collections::HashMap<String, BundleEntryResponse>>,
    (StatusCode, Json<ErrorResponse>),
> {
    let bundles = state.api.get_all_bundle_signers().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to get bundle signers: {}", e) }),
        )
    })?;

    let response = bundles
        .iter()
        .map(|(pubkey, bundle)| {
            (
                pubkey.to_string(),
                BundleEntryResponse {
                    pubkey: pubkey.to_string(),
                    tip: bundle.tip,
                    tx_count: bundle.tx_hashes.len(),
                    tx_hashes: bundle.tx_hashes.iter().map(|h| encode_tx_hash(h)).collect(),
                },
            )
        })
        .collect();

    Ok(Json(response))
}

// === Bundle Program Handlers ===

/// Add bundle program
async fn add_bundle_program(
    State(state): State<AppState>,
    Json(req): Json<AddBundleRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pubkey = Pubkey::from_str(&req.pubkey).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid pubkey: {}", e) }))
    })?;

    // Decode tx hashes
    let tx_hashes: Result<Vec<Vec<u8>>, String> =
        req.tx_hashes.iter().map(|h| decode_tx_hash(h)).collect();

    let tx_hashes =
        tx_hashes.map_err(|e| (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })))?;

    state
        .api
        .add_bundle_program(pubkey, req.tip, tx_hashes)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: format!("Failed to add bundle program: {}", e) }),
            )
        })?;

    Ok(Json(SuccessResponse {
        message: format!("Bundle program {} added successfully", req.pubkey),
    }))
}

/// Remove bundle program
async fn remove_bundle_program(
    State(state): State<AppState>,
    Path(pubkey_str): Path<String>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pubkey = Pubkey::from_str(&pubkey_str).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid pubkey: {}", e) }))
    })?;

    state.api.remove_bundle_program(pubkey).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to remove bundle program: {}", e) }),
        )
    })?;

    Ok(Json(SuccessResponse {
        message: format!("Bundle program {} removed successfully", pubkey_str),
    }))
}

/// Get bundle program
async fn get_bundle_program(
    State(state): State<AppState>,
    Path(pubkey_str): Path<String>,
) -> Result<Json<BundleEntryResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pubkey = Pubkey::from_str(&pubkey_str).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid pubkey: {}", e) }))
    })?;

    let bundle = state
        .api
        .get_bundle_program(pubkey)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: format!("Failed to get bundle program: {}", e) }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse { error: format!("Bundle program {} not found", pubkey_str) }),
            )
        })?;

    Ok(Json(BundleEntryResponse {
        pubkey: pubkey_str,
        tip: bundle.tip,
        tx_count: bundle.tx_hashes.len(),
        tx_hashes: bundle.tx_hashes.iter().map(|h| encode_tx_hash(h)).collect(),
    }))
}

/// Update bundle program tip
async fn update_bundle_program_tip(
    State(state): State<AppState>,
    Path(pubkey_str): Path<String>,
    Json(req): Json<UpdateBundleTipRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pubkey = Pubkey::from_str(&pubkey_str).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid pubkey: {}", e) }))
    })?;

    state
        .api
        .update_bundle_program_tip(pubkey, req.tip)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to update bundle program tip: {}", e),
                }),
            )
        })?;

    Ok(Json(SuccessResponse {
        message: format!("Bundle program {} tip updated to {}", pubkey_str, req.tip),
    }))
}

/// Add tx hashes to bundle program
async fn add_bundle_program_tx_hashes(
    State(state): State<AppState>,
    Path(pubkey_str): Path<String>,
    Json(req): Json<AddTxHashesRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pubkey = Pubkey::from_str(&pubkey_str).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid pubkey: {}", e) }))
    })?;

    // Decode tx hashes
    let tx_hashes: Result<Vec<Vec<u8>>, String> =
        req.tx_hashes.iter().map(|h| decode_tx_hash(h)).collect();

    let tx_hashes =
        tx_hashes.map_err(|e| (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })))?;

    state
        .api
        .add_bundle_program_tx_hashes(pubkey, tx_hashes.clone())
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to add tx hashes to bundle program: {}", e),
                }),
            )
        })?;

    Ok(Json(SuccessResponse {
        message: format!("Added {} tx hashes to bundle program {}", tx_hashes.len(), pubkey_str),
    }))
}

/// Get all bundle programs
async fn get_all_bundle_programs(
    State(state): State<AppState>,
) -> Result<
    Json<std::collections::HashMap<String, BundleEntryResponse>>,
    (StatusCode, Json<ErrorResponse>),
> {
    let bundles = state.api.get_all_bundle_programs().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to get bundle programs: {}", e) }),
        )
    })?;

    let response = bundles
        .iter()
        .map(|(pubkey, bundle)| {
            (
                pubkey.to_string(),
                BundleEntryResponse {
                    pubkey: pubkey.to_string(),
                    tip: bundle.tip,
                    tx_count: bundle.tx_hashes.len(),
                    tx_hashes: bundle.tx_hashes.iter().map(|h| encode_tx_hash(h)).collect(),
                },
            )
        })
        .collect();

    Ok(Json(response))
}

/// Clear all bundles
async fn clear_bundles(
    State(state): State<AppState>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    state.api.clear_bundles().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to clear bundles: {}", e) }),
        )
    })?;

    Ok(Json(SuccessResponse { message: "All bundles cleared successfully".to_string() }))
}

/// Creates the HTTP API router
pub fn create_router(api: SchedulerApi) -> Router {
    let state = AppState { api };

    Router::new()
        // Health check
        .route("/health", get(health))
        // Metrics endpoints
        .route("/metrics", get(get_all_metrics))
        .route("/metrics/signers", get(get_all_signer_stats))
        .route("/metrics/programs", get(get_all_program_stats))
        .route("/metrics/performance", get(get_performance_metrics))
        .route("/metrics/performance/reset", post(reset_performance_metrics))
        // Signer endpoints
        .route("/priority/signer", post(add_signer))
        .route("/priority/signer/{pubkey}", delete(remove_signer))
        .route("/priority/signer/{pubkey}/stats", get(get_signer_stats))
        .route("/priority/signer/{pubkey}/quota", get(get_signer_quota))
        .route("/priority/signer/{pubkey}/quota/increment", post(increment_signer_quota))
        // Program endpoints
        .route("/priority/program", post(add_program))
        .route("/priority/program/{pubkey}", delete(remove_program))
        .route("/priority/program/{pubkey}/stats", get(get_program_stats))
        .route("/priority/program/{pubkey}/quota", get(get_program_quota))
        .route("/priority/program/{pubkey}/quota/increment", post(increment_program_quota))
        // Bundle signer endpoints
        .route("/bundles/signer", post(add_bundle_signer))
        .route("/bundles/signer/{pubkey}", get(get_bundle_signer))
        .route("/bundles/signer/{pubkey}", delete(remove_bundle_signer))
        .route("/bundles/signer/{pubkey}/tip", post(update_bundle_signer_tip))
        .route("/bundles/signer/{pubkey}/tx-hashes", post(add_bundle_signer_tx_hashes))
        .route("/bundles/signers", get(get_all_bundle_signers))
        // Bundle program endpoints
        .route("/bundles/program", post(add_bundle_program))
        .route("/bundles/program/{pubkey}", get(get_bundle_program))
        .route("/bundles/program/{pubkey}", delete(remove_bundle_program))
        .route("/bundles/program/{pubkey}/tip", post(update_bundle_program_tip))
        .route("/bundles/program/{pubkey}/tx-hashes", post(add_bundle_program_tx_hashes))
        .route("/bundles/programs", get(get_all_bundle_programs))
        // Configuration endpoints
        .route("/priority/multiplier", post(set_multiplier))
        .route("/priority/clear", post(clear_lists))
        .route("/priority/clear/stats", post(clear_statistics))
        .route("/bundles/clear", post(clear_bundles))
        .with_state(state)
}

/// Starts the HTTP API server with graceful shutdown
pub async fn start_server(
    api: SchedulerApi,
    addr: SocketAddr,
    shutdown: toolbox::shutdown::Shutdown,
) -> Result<(), std::io::Error> {
    let app = create_router(api);

    tracing::info!("Starting HTTP API server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown.cancelled().await;
            tracing::info!("HTTP API server shutting down gracefully");
        })
        .await?;

    Ok(())
}
