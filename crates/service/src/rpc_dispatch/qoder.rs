use codexmanager_core::rpc::types::{JsonRpcRequest, JsonRpcResponse};

use crate::{
    qoder_events, qoder_overview, qoder_settings_result, qoder_status, qoder_today_top,
    save_qoder_settings, trigger_qoder_sync, QoderSettingsPatch,
};

pub(super) fn try_handle(req: &JsonRpcRequest) -> Option<JsonRpcResponse> {
    let result = match req.method.as_str() {
        "qoder/settingsGet" => super::value_or_error(qoder_settings_result()),
        "qoder/settingsSet" => {
            let patch = QoderSettingsPatch {
                base_url: super::string_param(req, "baseUrl"),
                organization_id: super::string_param(req, "organizationId"),
                api_key: super::string_param(req, "apiKey"),
                enabled: super::bool_param(req, "enabled"),
                backfill_days: super::i64_param(req, "backfillDays"),
            };
            super::value_or_error(save_qoder_settings(patch))
        }
        "qoder/status" => super::value_or_error(qoder_status()),
        "qoder/sync" => super::value_or_error(trigger_qoder_sync()),
        "qoder/overview" => {
            let start_day = super::string_param(req, "startDay");
            let end_day = super::string_param(req, "endDay");
            let member_id = super::string_param(req, "memberId");
            super::value_or_error(qoder_overview(start_day, end_day, member_id))
        }
        "qoder/events" => {
            let start_day = super::string_param(req, "startDay");
            let end_day = super::string_param(req, "endDay");
            let member_id = super::string_param(req, "memberId");
            let page = super::i64_param(req, "page");
            let page_size = super::i64_param(req, "pageSize");
            super::value_or_error(qoder_events(start_day, end_day, member_id, page, page_size))
        }
        "qoder/todayTop" => {
            super::value_or_error(qoder_today_top(super::i64_param(req, "limit")))
        }
        _ => return None,
    };

    Some(super::response(req, result))
}

#[cfg(test)]
mod tests {
    use super::try_handle;
    use codexmanager_core::rpc::types::JsonRpcRequest;

    fn rpc_request(method: &str, params: serde_json::Value) -> JsonRpcRequest {
        JsonRpcRequest {
            id: 1.into(),
            method: method.to_string(),
            params: Some(params),
            trace: None,
        }
    }

    #[test]
    fn unknown_method_falls_through() {
        assert!(try_handle(&rpc_request("qoder/unknown", serde_json::json!({}))).is_none());
        assert!(try_handle(&rpc_request("qoder/settingsGet", serde_json::json!({}))).is_some());
        assert!(try_handle(&rpc_request("qoder/overview", serde_json::json!({}))).is_some());
        assert!(try_handle(&rpc_request("qoder/events", serde_json::json!({}))).is_some());
        assert!(try_handle(&rpc_request("qoder/todayTop", serde_json::json!({}))).is_some());
    }
}
