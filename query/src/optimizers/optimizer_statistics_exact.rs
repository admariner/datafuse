// Copyright 2020 Datafuse Labs.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::sync::Arc;

use common_datavalues::DataValue;
use common_exception::Result;
use common_io::prelude::BinaryWrite;
use common_planners::AggregatorFinalPlan;
use common_planners::AggregatorPartialPlan;
use common_planners::Expression;
use common_planners::ExpressionPlan;
use common_planners::PlanBuilder;
use common_planners::PlanNode;
use common_planners::PlanRewriter;
use common_planners::TableScanInfo;

use crate::optimizers::Optimizer;
use crate::sessions::DatabendQueryContextRef;

struct StatisticsExactImpl<'a> {
    ctx: &'a DatabendQueryContextRef,
}

pub struct StatisticsExactOptimizer {
    ctx: DatabendQueryContextRef,
}

impl PlanRewriter for StatisticsExactImpl<'_> {
    fn rewrite_aggregate_partial(&mut self, plan: &AggregatorPartialPlan) -> Result<PlanNode> {
        let new_plan = match (
            &plan.group_expr[..],
            &plan.aggr_expr[..],
            plan.input.as_ref(),
        ) {
            (
                [],
                [Expression::AggregateFunction {
                    ref op,
                    distinct: false,
                    ref args,
                    ..
                }],
                PlanNode::Expression(ExpressionPlan { input, .. }),
            ) if op == "count" && args.len() == 1 => match (&args[0], input.as_ref()) {
                (Expression::Literal { .. }, PlanNode::ReadSource(read_source_plan))
                    if read_source_plan.statistics.is_exact =>
                {
                    let db_name = "system";
                    let table_name = "one";

                    let dummy_read_plan =
                        self.ctx
                            .get_table(db_name, table_name)
                            .and_then(|table_meta| {
                                let table = table_meta.raw();
                                let table_id = table_meta.meta_id();
                                let table_version = table_meta.meta_ver();

                                let tbl_scan_info = TableScanInfo {
                                    table_name,
                                    table_id,
                                    table_version,
                                    table_schema: &table.schema(),
                                    table_args: None,
                                };
                                PlanBuilder::scan(db_name, tbl_scan_info, None, None)
                                    .and_then(|builder| builder.build())
                                    .and_then(|dummy_scan_plan| match dummy_scan_plan {
                                        PlanNode::Scan(ref dummy_scan_plan) => {
                                            //
                                            let io_ctx =
                                                self.ctx.get_single_node_table_io_context()?;
                                            table
                                                .read_plan(
                                                    Arc::new(io_ctx),
                                                    Some(dummy_scan_plan.push_downs.clone()),
                                                    Some(
                                                        self.ctx.get_settings().get_max_threads()?
                                                            as usize,
                                                    ),
                                                )
                                                .map(PlanNode::ReadSource)
                                        }
                                        _unreachable_plan => {
                                            panic!("Logical error: cannot downcast to scan plan")
                                        }
                                    })
                            })?;
                    let mut body: Vec<u8> = Vec::new();
                    body.write_uvarint(read_source_plan.statistics.read_rows as u64)?;
                    let expr = Expression::create_literal(DataValue::String(Some(body)));
                    PlanBuilder::from(&dummy_read_plan)
                        .expression(&[expr.clone()], "Exact Statistics")?
                        .project(&[expr.alias("count(0)")])?
                        .build()?
                }
                _ => PlanNode::AggregatorPartial(plan.clone()),
            },
            (_, _, _) => PlanNode::AggregatorPartial(plan.clone()),
        };
        Ok(new_plan)
    }

    fn rewrite_aggregate_final(&mut self, plan: &AggregatorFinalPlan) -> Result<PlanNode> {
        Ok(PlanNode::AggregatorFinal(AggregatorFinalPlan {
            schema: plan.schema.clone(),
            schema_before_group_by: plan.schema_before_group_by.clone(),
            aggr_expr: plan.aggr_expr.clone(),
            group_expr: plan.group_expr.clone(),
            input: Arc::new(self.rewrite_plan_node(plan.input.as_ref())?),
        }))
    }
}

impl Optimizer for StatisticsExactOptimizer {
    fn name(&self) -> &str {
        "StatisticsExact"
    }

    fn optimize(&mut self, plan: &PlanNode) -> Result<PlanNode> {
        /*
            TODO:
                SELECT COUNT(1), COUNT(1) FROM (
                    SELECT COUNT(1) FROM (
                        SELECT * FROM system.settings LIMIT 1
                    )
                )
        */
        let mut visitor = StatisticsExactImpl { ctx: &self.ctx };
        visitor.rewrite_plan_node(plan)
    }
}

impl StatisticsExactOptimizer {
    pub fn create(ctx: DatabendQueryContextRef) -> Self {
        StatisticsExactOptimizer { ctx }
    }
}
