/*
 * Copyright 2024 Google LLC All Rights Reserved.
 *
 *  Licensed under the Apache License, Version 2.0 (the "License");
 *  you may not use this file except in compliance with the License.
 *  You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 *  Unless required by applicable law or agreed to in writing, software
 *  distributed under the License is distributed on an "AS IS" BASIS,
 *  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 *  See the License for the specific language governing permissions and
 *  limitations under the License.
 */

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
pub mod k8s;

const RETRIES: u32 = 25;
const BACKOFF_STEP: std::time::Duration = std::time::Duration::from_millis(250);
const MAX_DELAY: std::time::Duration = std::time::Duration::from_secs(2);

/// The available xDS source providers.
#[derive(Clone, Debug, clap::Args)]
pub struct Providers {
    /// Watches Agones' game server CRDs for `Allocated` game server endpoints,
    /// and for a `ConfigMap` that specifies the filter configuration.
    #[arg(
        long = "providers.k8s",
        env = "QUILKIN_PROVIDERS_K8S",
        default_value_t = false
    )]
    k8s_enabled: bool,

    #[arg(
        long = "providers.k8s.namespace",
        env = "QUILKIN_PROVIDERS_K8S_NAMESPACE",
        default_value_t = From::from("default"),
        requires("k8s_enabled"),
    )]
    k8s_namespace: String,

    #[arg(
        long = "providers.k8s.agones",
        env = "QUILKIN_PROVIDERS_K8S_AGONES",
        default_value_t = false
    )]
    agones_enabled: bool,

    #[arg(
        long = "providers.k8s.agones.namespace",
        env = "QUILKIN_PROVIDERS_K8S_AGONES_NAMESPACE",
        default_value_t = From::from("default"),
        requires("agones_enabled"),
    )]
    agones_namespace: String,

    /// If specified, filters the available gameserver addresses to the one that
    /// matches the specified type
    #[arg(
        long = "providers.k8s.agones.address_type",
        env = "QUILKIN_PROVIDERS_K8S_AGONES_ADDRESS_TYPE",
        requires("agones_enabled"),
    )]
    pub address_type: Option<String>,
    /// If specified, additionally filters the gameserver address by its ip kind
    #[arg(
        long = "providers.k8s.agones.ip_kind",
        env = "QUILKIN_PROVIDERS_K8S_AGONES_IP_KIND",
        requires("address_type"),
        value_enum,
    )]
    pub ip_kind: Option<crate::config::AddrKind>,

    #[arg(
        long = "providers.fs",
        env = "QUILKIN_PROVIDERS_FS",
        conflicts_with("k8s_enabled"),
        default_value_t = false
    )]
    fs_enabled: bool,

    #[arg(
        long = "providers.fs",
        env = "QUILKIN_PROVIDERS_FS_PATH",
        requires("fs_enabled"),
        default_value = "/etc/quilkin/config.yaml",
    )]
    fs_path: std::path::PathBuf,
    /// One or more `quilkin relay` endpoints to push configuration changes to.
    #[clap(long = "providers.mds.endpoints", env = "QUILKIN_PROVIDERS_MDS_ENDPOINTS")]
    pub relay: Vec<tonic::transport::Endpoint>,
    /// The remote URL or local file path to retrieve the Maxmind database.
    #[clap(long = "providers.mmdb.endpoints", env = "QUILKIN_PROVIDERS_MMDB_ENDPOINTS")]
    pub mmdb: Option<crate::net::maxmind_db::Source>,
    /// One or more socket addresses to forward packets to.
    #[clap(long = "providers.static.endpoints", env = "QUILKIN_PROVIDERS_STATIC_ENDPOINTS")]
    pub to: Vec<SocketAddr>,
    /// Assigns dynamic tokens to each address in the `--to` argument
    ///
    /// Format is `<number of unique tokens>:<length of token suffix for each packet>`
    #[clap(long, env = "QUILKIN_DEST_TOKENS", requires("to"))]
    #[clap(long = "providers.static.endpoint_tokens", env = "QUILKIN_PROVIDERS_STATIC_ENDPOINT_TOKENS", requires("to"))]
    pub to_tokens: Option<String>,
}

impl Providers {
    pub fn spawn_k8s_provider(&self) {
        let agones_namespace = self.agones_namespace.clone();
        let k8s_namespace = self.k8s_namespace.clone();
        let selector = self.address_type.map(|at| crate::config::AddressSelector {
            name: at,
            kind: self.ip_kind.unwrap_or(crate::config::AddrKind::Any),
        });

        tokio::spawn(async move {
            let client = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                kube::Client::try_default(),
            ).await??;

            Self::task(health_check.clone(), {
                let health_check = health_check.clone();
                move || {
                    crate::config::watch::agones(
                        agones_namespace,
                        k8s_namespace,
                        health_check.clone(),
                        locality.clone(),
                        config.clone(),
                        address_selector.clone(),
                    )
                }
            }).await
        })
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn spawn(
        self,
        config: std::sync::Arc<crate::Config>,
        health_check: Arc<AtomicBool>,
        locality: Option<crate::net::endpoint::Locality>,
        address_selector: Option<crate::config::AddressSelector>,
        is_agent: bool,
    ) -> tokio::task::JoinHandle<crate::Result<()>> {
        if self.k8s_enabled {
        } else if self.fs_enabled {
            tokio::spawn(Self::task(health_check.clone(), {
                let path = self.fs_path.clone();
                let health_check = health_check.clone();
                move || {
                    crate::config::watch::fs(
                        config.clone(),
                        health_check.clone(),
                        path.clone(),
                        locality.clone(),
                    )
                }
            }))
        } else {
            tokio::spawn(async move { Ok(()) })
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn task<F>(
        health_check: Arc<AtomicBool>,
        task: impl FnMut() -> F,
    ) -> crate::Result<()>
    where
        F: std::future::Future<Output = crate::Result<()>>,
    {
        tryhard::retry_fn(task)
            .retries(RETRIES)
            .exponential_backoff(BACKOFF_STEP)
            .max_delay(MAX_DELAY)
            .on_retry(|attempt, _, error: &eyre::Error| {
                health_check.store(false, Ordering::SeqCst);
                let error = error.to_string();
                async move {
                    tracing::warn!(%attempt, %error, "provider task error, retrying");
                }
            })
            .await
    }
}
