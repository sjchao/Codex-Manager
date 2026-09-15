use codexmanager_core::rpc::types::{JsonRpcRequest, JsonRpcResponse};
use crate::{delete_sub2api_account, list_sub2api_accounts, save_sub2api_account, sync_all_sub2api_accounts, sync_sub2api_account};

pub(super) fn try_handle(req: &JsonRpcRequest) -> Option<JsonRpcResponse> {
    let result = match req.method.as_str() {
        "sub2api/list" => super::value_or_error(list_sub2api_accounts()),
        "sub2api/create" | "sub2api/update" => super::value_or_error(save_sub2api_account(
            super::string_param(req, "id"),
            super::string_param(req, "baseUrl").unwrap_or_default(),
            super::string_param(req, "authToken"),
            super::string_param(req, "refreshToken"),
            super::i64_param(req, "tokenExpiresAt"),
        ).map(|id| serde_json::json!({"id": id}))),
        "sub2api/delete" => super::ok_or_error(delete_sub2api_account(super::str_param(req, "id").unwrap_or_default())),
        "sub2api/sync" => super::ok_or_error(sync_sub2api_account(super::str_param(req, "id").unwrap_or_default())),
        "sub2api/syncAll" => super::value_or_error(sync_all_sub2api_accounts()),
        _ => return None,
    };
    Some(super::response(req, result))
}
