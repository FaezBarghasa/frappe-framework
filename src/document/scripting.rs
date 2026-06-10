use rhai::{Engine, Scope, Dynamic, Map};
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::sync::Arc;
use frappe_meta::db::DatabaseClient;
use std::collections::HashMap;

pub struct ScriptSandbox {
    engine: Engine,
}

impl ScriptSandbox {
    pub fn new(db_client: Arc<DatabaseClient>) -> Self {
        let mut engine = Engine::new();

        // 1. Configure sandboxing limits
        engine.set_max_operations(5000); // Prevent infinite loops or heavy computations

        // 2. Register Database helper functions
        let db_clone = db_client.clone();
        engine.register_fn("db_get_value", move |doctype: String, _name: String, field: String| -> String {
            let db = db_clone.clone();
            // Since this is evaluated within a sync environment or we can block_on the async call
            let fut = async move {
                let fields = vec![field.as_str()];
                match db.get_list(&doctype, HashMap::new(), fields, 1, 0).await {
                    Ok(list) => {
                        if let Some(doc) = list.first() {
                            doc.get(&field)
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string()
                        } else {
                            "".to_string()
                        }
                    }
                    Err(_) => "".to_string(),
                }
            };
            
            // Execute the future synchronously using tokio block_in_place or block_on
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(fut)
            })
        });

        Self { engine }
    }

    /// Convert serde_json::Value to Rhai Dynamic
    fn json_to_dynamic(&self, val: JsonValue) -> Dynamic {
        match val {
            JsonValue::Null => Dynamic::UNIT,
            JsonValue::Bool(b) => Dynamic::from(b),
            JsonValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Dynamic::from(i)
                } else if let Some(f) = n.as_f64() {
                    Dynamic::from(f)
                } else {
                    Dynamic::UNIT
                }
            }
            JsonValue::String(s) => Dynamic::from(s),
            JsonValue::Array(arr) => {
                let dyn_arr: Vec<Dynamic> = arr.into_iter().map(|v| self.json_to_dynamic(v)).collect();
                Dynamic::from(dyn_arr)
            }
            JsonValue::Object(obj) => {
                let mut map = Map::new();
                for (k, v) in obj {
                    map.insert(k.into(), self.json_to_dynamic(v));
                }
                Dynamic::from(map)
            }
        }
    }

    /// Convert Rhai Dynamic back to serde_json::Value
    fn dynamic_to_json(&self, val: Dynamic) -> JsonValue {
        if val.is_unit() {
            JsonValue::Null
        } else if let Some(b) = val.clone().try_cast::<bool>() {
            JsonValue::Bool(b)
        } else if let Some(i) = val.clone().try_cast::<i64>() {
            JsonValue::Number(i.into())
        } else if let Some(f) = val.clone().try_cast::<f64>() {
            if let Some(num) = serde_json::Number::from_f64(f) {
                JsonValue::Number(num)
            } else {
                JsonValue::Null
            }
        } else if let Some(s) = val.clone().try_cast::<String>() {
            JsonValue::String(s)
        } else if let Some(arr) = val.clone().try_cast::<Vec<Dynamic>>() {
            JsonValue::Array(arr.into_iter().map(|v| self.dynamic_to_json(v)).collect())
        } else if let Some(map) = val.clone().try_cast::<Map>() {
            let mut obj = JsonMap::new();
            for (k, v) in map {
                obj.insert(k.to_string(), self.dynamic_to_json(v));
            }
            JsonValue::Object(obj)
        } else {
            JsonValue::Null
        }
    }

    /// Evaluates a Rhai script to modify a document dynamically in a sandbox.
    pub fn execute_script(
        &self,
        script: &str,
        doc: JsonMap<String, JsonValue>,
    ) -> Result<JsonMap<String, JsonValue>, String> {
        let mut scope = Scope::new();

        // Convert input document to Rhai Dynamic map
        let doc_dynamic = self.json_to_dynamic(JsonValue::Object(doc));
        scope.push("doc", doc_dynamic);

        // Compile and run the script
        let ast = self.engine.compile(script).map_err(|e| format!("Script Compilation Error: {}", e))?;
        
        self.engine.run_ast_with_scope(&mut scope, &ast)
            .map_err(|e| format!("Script Runtime Error: {}", e))?;

        // Extract the mutated document
        let final_doc = scope.get_value::<Dynamic>("doc")
            .ok_or_else(|| "doc variable was removed or cleared by the script".to_string())?;

        let json_val = self.dynamic_to_json(final_doc);
        if let JsonValue::Object(map) = json_val {
            Ok(map)
        } else {
            Err("doc was mutated to a non-object type".to_string())
        }
    }
}
