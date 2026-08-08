//! 辅助方法测试：normalized_output / get_output / heating_output / literal 默认值。
//! 公式迁移自 metatorio-egui（ItemResult::normalized_output、GeneratorPrototype::get_output、
//! BoilerPrototype::get_flow 核心公式）。

use metatorio_data::types::{EnergyAmount, FluidProduct, ItemProduct, ProbabilityInfo, Production};
use metatorio_data::*;

fn energy(j: f64) -> EnergyAmount {
    EnergyAmount { amount: j }
}

fn fluid(default_temperature: f64, heat_capacity: f64, fuel_value: f64) -> FluidComponent {
    FluidComponent {
        default_temperature,
        max_temperature: Some(1000.0),
        heat_capacity: Some(energy(heat_capacity)),
        fuel_value: Some(energy(fuel_value)),
        ..Default::default()
    }
}

#[test]
fn item_product_normalized_output() {
    // 确定产量：amount=2，概率 1
    let p = ItemProduct {
        amount: Some(2),
        ..Default::default()
    };
    let o = p.normalized_output();
    // productivity 与 metatorio-egui 公式一致：prob=1 时 = base（忠实迁移，语义待用户评估）
    assert_eq!(
        o,
        Production {
            base: 2.0,
            productivity: 2.0
        }
    );

    // 概率 50%：base = 2 * 0.5 = 1.0，productivity = 2*0.5*1 = 1.0
    let p = ItemProduct {
        amount: Some(2),
        probability_info: ProbabilityInfo {
            independent_probability: 0.5,
            ..Default::default()
        },
        ..Default::default()
    };
    let o = p.normalized_output();
    assert_eq!(
        o,
        Production {
            base: 1.0,
            productivity: 1.0
        }
    );

    // extra_count_fraction：amount=2、prob=1、extra=0.5
    let p = ItemProduct {
        amount: Some(2),
        extra_count_fraction: 0.5,
        ..Default::default()
    };
    let o = p.normalized_output();
    assert_eq!(
        o,
        Production {
            base: 2.5,
            productivity: 2.5
        }
    );

    // 数量区间 [2, 4]、概率 1：base = 3.0
    let p = ItemProduct {
        amount: None,
        amount_min: Some(2),
        amount_max: Some(4),
        ..Default::default()
    };
    let o = p.normalized_output();
    assert_eq!(
        o,
        Production {
            base: 3.0,
            productivity: 3.0
        }
    );
}

#[test]
fn fluid_product_normalized_output() {
    let p = FluidProduct {
        amount: Some(3.0),
        ..Default::default()
    };
    let o = p.normalized_output();
    assert_eq!(
        o,
        Production {
            base: 3.0,
            productivity: 3.0
        }
    );

    // 区间 [1, 5]：base = 3.0，productivity = 6*4/2/4 = 3.0
    let p = FluidProduct {
        amount: None,
        amount_min: Some(1.0),
        amount_max: Some(5.0),
        ..Default::default()
    };
    let o = p.normalized_output();
    assert_eq!(
        o,
        Production {
            base: 3.0,
            productivity: 3.0
        }
    );
}

#[test]
fn generator_burns_fluid() {
    let g = GeneratorComponent {
        burns_fluid: true,
        fluid_usage_per_tick: 1.0,
        effectivity: 1.0,
        ..Default::default()
    };
    // 燃料 5MJ/unit：功率 = 1 * 5e6 * 1 * 60 = 3e8 W
    let o = g.get_output("water", &fluid(15.0, 1000.0, 5_000_000.0), 15.0);
    assert_eq!(o.fluid_used_per_second, 60.0);
    assert_eq!(o.power_per_second, 300_000_000.0);
}

#[test]
fn generator_power_capped() {
    let g = GeneratorComponent {
        burns_fluid: true,
        fluid_usage_per_tick: 1.0,
        effectivity: 1.0,
        scale_fluid_usage: true,
        max_power_output: Some(energy(1_000_000.0)),
        ..Default::default()
    };
    let o = g.get_output("water", &fluid(15.0, 1000.0, 5_000_000.0), 15.0);
    // scale = 1e6 / 5e6 = 0.2：流体 60 * 0.2 = 12/s，功率 1e6 * 60 = 6e7
    assert_eq!(o.fluid_used_per_second, 12.0);
    assert_eq!(o.power_per_second, 60_000_000.0);
}

#[test]
fn generator_temperature_diff() {
    let g = GeneratorComponent {
        burns_fluid: false,
        fluid_usage_per_tick: 1.0,
        effectivity: 1.0,
        maximum_temperature: 500.0,
        destroy_non_fuel_fluid: true,
        fluid_box: FluidBox {
            filter: Some("water".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let f = fluid(15.0, 1000.0, 0.0);
    // ΔT = 100：功率 = 100 * 1 * 1000 * 1 * 60 = 6e6 W
    let o = g.get_output("water", &f, 115.0);
    assert_eq!(o.fluid_used_per_second, 60.0);
    assert_eq!(o.power_per_second, 6_000_000.0);

    // 温度不足（ΔT = 0）：功率 0
    let o = g.get_output("water", &f, 15.0);
    assert_eq!(o.power_per_second, 0.0);

    // 过滤不匹配：流体盒拒绝该流体，不消耗也不产电。
    let o = g.get_output("steam", &f, 115.0);
    assert_eq!(o.fluid_used_per_second, 0.0);
    assert_eq!(o.power_per_second, 0.0);
}

#[test]
fn boiler_heating_output() {
    let b = BoilerComponent {
        energy_consumption: energy(1_000_000.0),
        target_temperature: Some(165.0),
        ..Default::default()
    };
    let f = fluid(15.0, 1000.0, 0.0);
    // amount = 1e6 * 60 / 1000 / (165-15) = 400 unit/s
    let o = b.heating_output(&f, &f, 15.0).unwrap();
    assert_eq!(o.input_amount_per_second, 400.0);
    assert_eq!(o.output_amount_per_second, 400.0);
    assert_eq!(o.output_temperature, 165.0);

    // 温度已到目标 → None
    assert!(b.heating_output(&f, &f, 165.0).is_none());

    // 输出流体比热容不同：output = 400 * 1000 / 2000 = 200
    let heavy = fluid(15.0, 2000.0, 0.0);
    let o = b.heating_output(&f, &heavy, 15.0).unwrap();
    assert_eq!(o.output_amount_per_second, 200.0);
}

#[test]
fn literal_default_energy() {
    // 未设置 → literal 默认
    let f = FluidComponent::default();
    assert_eq!(f.heat_capacity(), energy(1000.0));
    assert_eq!(f.fuel_value(), energy(0.0));

    let e = EntityComponent::default();
    assert_eq!(e.heating_energy(), energy(0.0));

    let i = ItemComponent::default();
    assert_eq!(i.fuel_value(), energy(0.0));

    let l = LoaderComponent::default();
    assert_eq!(l.energy_per_item(), energy(0.0));

    // 已设置 → 字段值优先
    let mut f = FluidComponent::default();
    f.heat_capacity = Some(energy(2000.0));
    assert_eq!(f.heat_capacity(), energy(2000.0));
}
