use anyhow::Result;
use serde_json::Value;

pub fn remove_outline_panel_settings(settings: &mut Value) -> Result<()> {
    if let Some(object) = settings.as_object_mut() {
        object.remove("outline_panel");
    }

    Ok(())
}
