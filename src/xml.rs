use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub documento: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub numero: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pedido: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cliente: String,
}

/// Extracción best-effort: un XML inválido no es error.
pub fn extract(content: &[u8]) -> (String, Vec<Document>) {
    let Ok(text) = std::str::from_utf8(content) else {
        return (String::new(), Vec::new());
    };
    let Ok(doc) = roxmltree::Document::parse(text) else {
        return (String::new(), Vec::new());
    };
    let mut company = String::new();
    let mut documents = Vec::new();
    for node in doc.descendants() {
        if node.tag_name().name() != "OPERATION" {
            continue;
        }
        let Some(parent) = node.parent() else {
            continue;
        };
        if parent.tag_name().name() != "OPERATIONS" {
            continue;
        }
        let value = |name: &str| -> String {
            node.children()
                .find(|c| c.is_element() && c.tag_name().name() == name)
                .and_then(|c| c.text())
                .map(|t| t.trim().to_string())
                .unwrap_or_default()
        };
        let company_id = value("COMPANYID");
        if company.is_empty() && !company_id.is_empty() {
            company = company_id;
        }
        documents.push(Document {
            documento: value("DOCUMENTID"),
            numero: value("NUMBERID"),
            pedido: value("PURCHASEORDER"),
            cliente: value("CLIENTID"),
        });
    }
    (company, documents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_documents() {
        let xml = br#"<ROWS><OPERATIONS><OPERATION>
            <COMPANYID>1</COMPANYID><DOCUMENTID>PD</DOCUMENTID>
            <NUMBERID>0</NUMBERID><CLIENTID>2</CLIENTID>
            <PURCHASEORDER>PEDIDO-001</PURCHASEORDER>
        </OPERATION></OPERATIONS></ROWS>"#;
        let (company, docs) = extract(xml);
        assert_eq!(company, "1");
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].documento, "PD");
        assert_eq!(docs[0].numero, "0");
        assert_eq!(docs[0].pedido, "PEDIDO-001");
        assert_eq!(docs[0].cliente, "2");
    }

    #[test]
    fn tolerates_namespaces_and_garbage() {
        let xml = br#"<r:ROWS xmlns:r="urn:x"><r:OPERATIONS><r:OPERATION>
            <r:COMPANYID> 7 </r:COMPANYID></r:OPERATION></r:OPERATIONS></r:ROWS>"#;
        let (company, docs) = extract(xml);
        assert_eq!(company, "7");
        assert_eq!(docs.len(), 1);
        assert_eq!(extract(b"<ROWS>"), (String::new(), Vec::new()));
    }
}
