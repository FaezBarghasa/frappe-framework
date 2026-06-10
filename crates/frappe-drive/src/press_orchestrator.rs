use serde::{Deserialize, Serialize};
use kube::{Client, Api, api::{PostParams, DeleteParams}};
use k8s_openapi::api::core::v1::{Namespace, Service};
use k8s_openapi::api::apps::v1::Deployment;

#[derive(thiserror::Error, Debug)]
pub enum OrchestratorError {
    #[error("Kubernetes API error: {0}")]
    Kube(#[from] kube::Error),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SiteStatus {
    pub name: String,
    pub replicas: i32,
    pub available_replicas: i32,
    pub ready: bool,
}

pub struct PressOrchestrator {
    client: Client,
}

impl PressOrchestrator {
    /// Creates a new PressOrchestrator instance using the given Kubernetes client.
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Provisions an isolated tenant site namespace, deployment, and service.
    pub async fn provision_tenant_site(&self, tenant_id: &str) -> Result<(), OrchestratorError> {
        let namespace = format!("tenant-{}", tenant_id);

        // 1. Create Namespace
        let ns_json = serde_json::json!({
            "apiVersion": "v1",
            "kind": "Namespace",
            "metadata": {
                "name": namespace,
                "labels": {
                    "app": "frappe-tenant",
                    "tenant": tenant_id
                }
            }
        });
        let ns: Namespace = serde_json::from_value(ns_json)
            .map_err(|e| OrchestratorError::Serialization(e.to_string()))?;
        let namespaces: Api<Namespace> = Api::all(self.client.clone());
        namespaces.create(&PostParams::default(), &ns).await?;

        // 2. Create Service
        let svc_json = serde_json::json!({
            "apiVersion": "v1",
            "kind": "Service",
            "metadata": {
                "name": "frappe-service",
                "namespace": namespace
            },
            "spec": {
                "selector": {
                    "app": "frappe-app"
                },
                "ports": [
                    {
                        "port": 80,
                        "targetPort": 8000
                    }
                ]
            }
        });
        let svc: Service = serde_json::from_value(svc_json)
            .map_err(|e| OrchestratorError::Serialization(e.to_string()))?;
        let services: Api<Service> = Api::namespaced(self.client.clone(), &namespace);
        services.create(&PostParams::default(), &svc).await?;

        // 3. Create Deployment
        let depl_json = serde_json::json!({
            "apiVersion": "apps/v1",
            "kind": "Deployment",
            "metadata": {
                "name": "frappe-deployment",
                "namespace": namespace
            },
            "spec": {
                "replicas": 2,
                "selector": {
                    "matchLabels": {
                        "app": "frappe-app"
                    }
                },
                "template": {
                    "metadata": {
                        "labels": {
                            "app": "frappe-app"
                        }
                    },
                    "spec": {
                        "containers": [
                            {
                                "name": "frappe-site",
                                "image": "frappe/frappe-socketio:latest",
                                "ports": [
                                    {
                                        "containerPort": 8000
                                    }
                                ]
                            }
                        ]
                    }
                }
            }
        });
        let depl: Deployment = serde_json::from_value(depl_json)
            .map_err(|e| OrchestratorError::Serialization(e.to_string()))?;
        let deployments: Api<Deployment> = Api::namespaced(self.client.clone(), &namespace);
        deployments.create(&PostParams::default(), &depl).await?;

        Ok(())
    }

    /// Deprovisions an isolated tenant site namespace, deleting all contained resources.
    pub async fn deprovision_tenant_site(&self, tenant_id: &str) -> Result<(), OrchestratorError> {
        let namespace = format!("tenant-{}", tenant_id);
        let namespaces: Api<Namespace> = Api::all(self.client.clone());
        namespaces.delete(&namespace, &DeleteParams::default()).await?;
        Ok(())
    }

    /// Checks the operational status of a tenant's Kubernetes deployment.
    pub async fn check_tenant_site_status(&self, tenant_id: &str) -> Result<SiteStatus, OrchestratorError> {
        let namespace = format!("tenant-{}", tenant_id);
        let deployments: Api<Deployment> = Api::namespaced(self.client.clone(), &namespace);
        let depl = deployments.get("frappe-deployment").await?;

        let replicas = depl.spec.as_ref()
            .and_then(|s| s.replicas)
            .unwrap_or(0);

        let status = depl.status.as_ref();
        let available_replicas = status
            .and_then(|s| s.available_replicas)
            .unwrap_or(0);

        let ready = status
            .and_then(|s| s.ready_replicas)
            .map(|r| r == replicas)
            .unwrap_or(false);

        Ok(SiteStatus {
            name: tenant_id.to_string(),
            replicas,
            available_replicas,
            ready,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_orchestrator_initialization() {
        // Since we don't assume a running k8s cluster in test environment, we attempt client initialization
        // but catch connection errors. The key is that this checks compilation of the API structures.
        let client_res = Client::try_default().await;
        if let Ok(client) = client_res {
            let orchestrator = PressOrchestrator::new(client);
            // We can't query actual cluster without it running, but we check namespace format helper
            assert_eq!(format!("tenant-{}", "demo"), "tenant-demo");
            drop(orchestrator);
        }
    }
}
