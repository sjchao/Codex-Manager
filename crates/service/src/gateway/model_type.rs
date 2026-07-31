#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModelType {
    Text,
    Image,
    Video,
}

impl ModelType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::Video => "video",
        }
    }
}

pub(crate) fn classify_model_for_gateway_settings(model: Option<&str>) -> ModelType {
    let image_models = crate::app_settings::current_gateway_image_model_list();
    let video_models = crate::app_settings::current_gateway_video_model_list();
    classify_model_for_lists(model, &image_models, &video_models)
}

pub(crate) fn classify_model_for_lists(
    model: Option<&str>,
    image_models: &[String],
    video_models: &[String],
) -> ModelType {
    let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) else {
        return ModelType::Text;
    };

    if image_models
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(model))
    {
        ModelType::Image
    } else if video_models
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(model))
    {
        ModelType::Video
    } else {
        ModelType::Text
    }
}

#[cfg(test)]
mod tests {
    use super::{classify_model_for_lists, ModelType};

    #[test]
    fn model_types_have_stable_storage_strings() {
        assert_eq!(ModelType::Text.as_str(), "text");
        assert_eq!(ModelType::Image.as_str(), "image");
        assert_eq!(ModelType::Video.as_str(), "video");
    }

    #[test]
    fn classify_model_uses_exact_case_insensitive_configured_lists() {
        let image_models = vec!["gpt-image2".to_string()];
        let video_models = vec!["sora-2".to_string()];

        assert_eq!(
            classify_model_for_lists(Some("GPT-IMAGE2"), &image_models, &video_models),
            ModelType::Image
        );
        assert_eq!(
            classify_model_for_lists(Some("sora-2"), &image_models, &video_models),
            ModelType::Video
        );
        assert_eq!(
            classify_model_for_lists(Some("gpt-image2-preview"), &image_models, &video_models),
            ModelType::Text
        );
        assert_eq!(
            classify_model_for_lists(Some("   "), &image_models, &video_models),
            ModelType::Text
        );
    }
}
