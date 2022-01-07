use crate::utxo::utxo_builder::{UtxoCoinBuildError, UtxoCoinBuildHwOps, UtxoCoinBuilder, UtxoCoinBuilderCommonOps,
                                UtxoCoinWithIguanaPrivKeyBuilder, UtxoFieldsWithHardwareWalletBuilder,
                                UtxoFieldsWithIguanaPrivKeyBuilder};
use crate::utxo::utxo_common::merge_utxo_loop;
use crate::utxo::{UtxoArc, UtxoCoinFields, UtxoCommonOps, UtxoWeak};
use crate::{PrivKeyBuildPolicy, UtxoActivationParams};
use async_trait::async_trait;
use common::executor::spawn;
use common::log::info;
use common::mm_ctx::MmArc;
use common::mm_error::prelude::*;
use serde_json::Value as Json;

pub struct UtxoArcBuilder<'a, F, T, HwOps>
where
    F: Fn(UtxoArc) -> T + Clone + Send + Sync + 'static,
    HwOps: UtxoCoinBuildHwOps + Send + Sync,
{
    ctx: &'a MmArc,
    ticker: &'a str,
    conf: &'a Json,
    activation_params: UtxoActivationParams,
    priv_key_policy: PrivKeyBuildPolicy<'a>,
    hw_ops: HwOps,
    constructor: F,
}

impl<'a, F, T, HwOps> UtxoArcBuilder<'a, F, T, HwOps>
where
    F: Fn(UtxoArc) -> T + Clone + Send + Sync + 'static,
    HwOps: UtxoCoinBuildHwOps + Send + Sync,
{
    pub fn new(
        ctx: &'a MmArc,
        ticker: &'a str,
        conf: &'a Json,
        activation_params: UtxoActivationParams,
        priv_key_policy: PrivKeyBuildPolicy<'a>,
        hw_ops: HwOps,
        constructor: F,
    ) -> UtxoArcBuilder<'a, F, T, HwOps> {
        UtxoArcBuilder {
            ctx,
            ticker,
            conf,
            activation_params,
            priv_key_policy,
            hw_ops,
            constructor,
        }
    }
}

#[async_trait]
impl<'a, F, T, HwOps> UtxoCoinBuilderCommonOps for UtxoArcBuilder<'a, F, T, HwOps>
where
    F: Fn(UtxoArc) -> T + Clone + Send + Sync + 'static,
    HwOps: UtxoCoinBuildHwOps + Send + Sync,
{
    fn ctx(&self) -> &MmArc { self.ctx }

    fn conf(&self) -> &Json { self.conf }

    fn activation_params(&self) -> UtxoActivationParams { self.activation_params.clone() }

    fn ticker(&self) -> &str { self.ticker }
}

impl<'a, F, T, HwOps> UtxoFieldsWithIguanaPrivKeyBuilder for UtxoArcBuilder<'a, F, T, HwOps>
where
    F: Fn(UtxoArc) -> T + Clone + Send + Sync + 'static,
    HwOps: UtxoCoinBuildHwOps + Send + Sync,
{
}

#[async_trait]
impl<'a, F, T, HwOps> UtxoFieldsWithHardwareWalletBuilder<HwOps> for UtxoArcBuilder<'a, F, T, HwOps>
where
    F: Fn(UtxoArc) -> T + Clone + Send + Sync + 'static,
    HwOps: UtxoCoinBuildHwOps + Send + Sync,
{
}

#[async_trait]
impl<'a, F, T, HwOps> UtxoCoinBuilder<HwOps> for UtxoArcBuilder<'a, F, T, HwOps>
where
    F: Fn(UtxoArc) -> T + Clone + Send + Sync + 'static,
    T: AsRef<UtxoCoinFields> + UtxoCommonOps + Send + Sync + 'static,
    HwOps: UtxoCoinBuildHwOps + Send + Sync,
{
    type ResultCoin = T;
    type Error = UtxoCoinBuildError;

    fn priv_key_policy(&self) -> PrivKeyBuildPolicy<'_> { self.priv_key_policy.clone() }

    fn hw_ops(&self) -> &HwOps { &self.hw_ops }

    async fn build(self) -> MmResult<Self::ResultCoin, Self::Error> {
        let utxo = self.build_utxo_fields().await?;
        let utxo_arc = UtxoArc::new(utxo);
        let utxo_weak = utxo_arc.downgrade();
        let result_coin = (self.constructor)(utxo_arc);

        self.spawn_merge_utxo_loop_if_required(utxo_weak);
        Ok(result_coin)
    }
}

impl<'a, F, T, HwOps> MergeUtxoArcOps<F, T> for UtxoArcBuilder<'a, F, T, HwOps>
where
    F: Fn(UtxoArc) -> T + Clone + Send + Sync + 'static,
    T: AsRef<UtxoCoinFields> + UtxoCommonOps + Send + Sync + 'static,
    HwOps: UtxoCoinBuildHwOps + Send + Sync,
{
    fn constructor(&self) -> F { self.constructor.clone() }
}

pub struct UtxoArcWithIguanaPrivKeyBuilder<'a, F, T>
where
    F: Fn(UtxoArc) -> T + Clone + Send + Sync + 'static,
{
    ctx: &'a MmArc,
    ticker: &'a str,
    conf: &'a Json,
    activation_params: UtxoActivationParams,
    priv_key: &'a [u8],
    constructor: F,
}

#[async_trait]
impl<'a, F, T> UtxoCoinBuilderCommonOps for UtxoArcWithIguanaPrivKeyBuilder<'a, F, T>
where
    F: Fn(UtxoArc) -> T + Clone + Send + Sync + 'static,
{
    fn ctx(&self) -> &MmArc { self.ctx }

    fn conf(&self) -> &Json { self.conf }

    fn activation_params(&self) -> UtxoActivationParams { self.activation_params.clone() }

    fn ticker(&self) -> &str { self.ticker }
}

#[async_trait]
impl<'a, F, T> UtxoFieldsWithIguanaPrivKeyBuilder for UtxoArcWithIguanaPrivKeyBuilder<'a, F, T> where
    F: Fn(UtxoArc) -> T + Clone + Send + Sync + 'static
{
}

impl<'a, F, T> MergeUtxoArcOps<F, T> for UtxoArcWithIguanaPrivKeyBuilder<'a, F, T>
where
    F: Fn(UtxoArc) -> T + Clone + Send + Sync + 'static,
    T: AsRef<UtxoCoinFields> + UtxoCommonOps + Send + Sync + 'static,
{
    fn constructor(&self) -> F { self.constructor.clone() }
}

#[async_trait]
impl<'a, F, T> UtxoCoinWithIguanaPrivKeyBuilder for UtxoArcWithIguanaPrivKeyBuilder<'a, F, T>
where
    F: Fn(UtxoArc) -> T + Clone + Send + Sync + 'static,
    T: AsRef<UtxoCoinFields> + UtxoCommonOps + Send + Sync + 'static,
{
    type ResultCoin = T;
    type Error = UtxoCoinBuildError;

    fn priv_key(&self) -> &[u8] { self.priv_key }

    async fn build(self) -> MmResult<Self::ResultCoin, Self::Error> {
        let utxo = self.build_utxo_fields_with_iguana_priv_key(self.priv_key()).await?;
        let utxo_arc = UtxoArc::new(utxo);
        let utxo_weak = utxo_arc.downgrade();
        let result_coin = (self.constructor)(utxo_arc);

        self.spawn_merge_utxo_loop_if_required(utxo_weak);
        Ok(result_coin)
    }
}

impl<'a, F, T> UtxoArcWithIguanaPrivKeyBuilder<'a, F, T>
where
    F: Fn(UtxoArc) -> T + Clone + Send + Sync + 'static,
    T: AsRef<UtxoCoinFields> + UtxoCommonOps + Send + Sync + 'static,
{
    pub fn new(
        ctx: &'a MmArc,
        ticker: &'a str,
        conf: &'a Json,
        activation_params: UtxoActivationParams,
        priv_key: &'a [u8],
        constructor: F,
    ) -> UtxoArcWithIguanaPrivKeyBuilder<'a, F, T> {
        UtxoArcWithIguanaPrivKeyBuilder {
            ctx,
            ticker,
            conf,
            activation_params,
            priv_key,
            constructor,
        }
    }
}

trait MergeUtxoArcOps<F, T>: UtxoCoinBuilderCommonOps
where
    F: Fn(UtxoArc) -> T + Clone + Send + Sync + 'static,
    T: AsRef<UtxoCoinFields> + UtxoCommonOps + Send + Sync + 'static,
{
    fn constructor(&self) -> F;

    fn spawn_merge_utxo_loop_if_required(&self, weak: UtxoWeak) {
        if let Some(merge_params) = self.activation_params().utxo_merge_params {
            let fut = merge_utxo_loop(
                weak,
                merge_params.merge_at,
                merge_params.check_every,
                merge_params.max_merge_at_once,
                self.constructor(),
            );
            info!("Starting UTXO merge loop for coin {}", self.ticker());
            spawn(fut);
        }
    }
}
