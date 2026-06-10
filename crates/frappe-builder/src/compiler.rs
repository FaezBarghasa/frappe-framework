use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(thiserror::Error, Debug)]
pub enum CompileError {
    #[error("Unsafe HTML tag blocked: {0}")]
    UnsafeTag(String),
    #[error("Unsafe attribute or value blocked: attribute '{0}', value '{1}'")]
    UnsafeAttribute(String, String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayoutNode {
    pub tag: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub attributes: HashMap<String, String>,
    pub children: Option<Vec<LayoutNode>>,
    pub text: Option<String>,
}

/// Check if the HTML tag is on our strict allow-list.
fn is_safe_tag(tag: &str) -> bool {
    let lower = tag.to_lowercase();
    matches!(
        lower.as_str(),
        "div" | "span" | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" |
        "button" | "input" | "label" | "form" | "table" | "thead" | "tbody" |
        "tr" | "th" | "td" | "img" | "a" | "section" | "header" | "footer" |
        "nav" | "main" | "aside" | "ul" | "ol" | "li" | "br" | "hr" |
        "svg" | "path" | "select" | "option" | "textarea"
    )
}

/// Validate that attribute keys do not contain event handlers and values do not contain script injection vectors.
fn is_safe_attribute(name: &str, value: &str) -> bool {
    let name_lower = name.to_lowercase();
    if name_lower.starts_with("on") {
        return false;
    }
    if name_lower == "href" || name_lower == "src" {
        let val_lower = value.to_lowercase().trim().to_string();
        if val_lower.starts_with("javascript:") || val_lower.starts_with("data:") {
            return false;
        }
    }
    true
}

/// Standard HTML entity escaping to prevent XSS.
fn escape_html(input: &str) -> String {
    let mut escaped = String::new();
    for c in input.chars() {
        match c {
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '&' => escaped.push_str("&amp;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#x27;"),
            '/' => escaped.push_str("&#x2F;"),
            _ => escaped.push(c),
        }
    }
    escaped
}

/// Compiles a JSON-compatible LayoutNode AST into static HTML with Tailwind utility classes.
pub fn compile_ast(node: &LayoutNode) -> Result<String, CompileError> {
    if !is_safe_tag(&node.tag) {
        return Err(CompileError::UnsafeTag(node.tag.clone()));
    }

    let mut html = format!("<{}", node.tag);

    if let Some(ref id) = node.id {
        html.push_str(&format!(" id=\"{}\"", escape_html(id)));
    }

    if !node.classes.is_empty() {
        let class_str = node.classes.join(" ");
        html.push_str(&format!(" class=\"{}\"", escape_html(&class_str)));
    }

    // Sort attributes to have deterministic output for testing
    let mut attrs: Vec<(&String, &String)> = node.attributes.iter().collect();
    attrs.sort_by(|a, b| a.0.cmp(b.0));

    for (name, value) in attrs {
        if !is_safe_attribute(name, value) {
            return Err(CompileError::UnsafeAttribute(name.clone(), value.clone()));
        }
        html.push_str(&format!(" {}=\"{}\"", escape_html(name), escape_html(value)));
    }

    let is_self_closing = matches!(node.tag.to_lowercase().as_str(), "img" | "br" | "hr" | "input");

    if is_self_closing {
        html.push_str(" />");
    } else {
        html.push('>');

        if let Some(ref text) = node.text {
            html.push_str(&escape_html(text));
        }

        if let Some(ref children) = node.children {
            for child in children {
                html.push_str(&compile_ast(child)?);
            }
        }

        html.push_str(&format!("</{}>", node.tag));
    }

    Ok(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_basic_node() {
        let mut attributes = HashMap::new();
        attributes.insert("type".to_string(), "text".to_string());
        
        let node = LayoutNode {
            tag: "input".to_string(),
            id: Some("user_input".to_string()),
            classes: vec!["w-full".to_string(), "p-2".to_string()],
            attributes,
            children: None,
            text: None,
        };

        let result = compile_ast(&node).unwrap();
        assert!(result.contains("<input"));
        assert!(result.contains("id=\"user_input\""));
        assert!(result.contains("class=\"w-full p-2\""));
        assert!(result.contains("type=\"text\""));
    }

    #[test]
    fn test_compile_nested_elements() {
        let parent = LayoutNode {
            tag: "div".to_string(),
            id: None,
            classes: vec!["flex".to_string()],
            attributes: HashMap::new(),
            children: Some(vec![
                LayoutNode {
                    tag: "p".to_string(),
                    id: None,
                    classes: vec![],
                    attributes: HashMap::new(),
                    children: None,
                    text: Some("Hello World".to_string()),
                }
            ]),
            text: None,
        };

        let result = compile_ast(&parent).unwrap();
        assert_eq!(result, "<div class=\"flex\"><p>Hello World</p></div>");
    }

    #[test]
    fn test_blocked_unsafe_tags() {
        let bad_node = LayoutNode {
            tag: "script".to_string(),
            id: None,
            classes: vec![],
            attributes: HashMap::new(),
            children: None,
            text: Some("alert(1)".to_string()),
        };

        let result = compile_ast(&bad_node);
        assert!(result.is_err());
    }

    #[test]
    fn test_blocked_unsafe_attributes() {
        let mut attributes = HashMap::new();
        attributes.insert("onclick".to_string(), "alert(1)".to_string());

        let node = LayoutNode {
            tag: "button".to_string(),
            id: None,
            classes: vec![],
            attributes,
            children: None,
            text: Some("Click".to_string()),
        };

        let result = compile_ast(&node);
        assert!(result.is_err());
    }

    #[test]
    fn test_blocked_javascript_urls() {
        let mut attributes = HashMap::new();
        attributes.insert("href".to_string(), "javascript:alert(1)".to_string());

        let node = LayoutNode {
            tag: "a".to_string(),
            id: None,
            classes: vec![],
            attributes,
            children: None,
            text: Some("Link".to_string()),
        };

        let result = compile_ast(&node);
        assert!(result.is_err());
    }
}
