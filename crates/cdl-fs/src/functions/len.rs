use std::{any::Any, sync::Arc};

use deltalake::{
    arrow::{
        array::{Array, AsArray, Int64Array},
        datatypes::DataType,
    },
    datafusion::{
        error::Result,
        logical_expr::{
            ColumnarValue, ScalarUDF, ScalarUDFImpl, Signature, TypeSignature, Volatility,
        },
        scalar::ScalarValue,
    },
};
use itertools::Itertools;

#[derive(Debug)]
pub(crate) struct Udf {
    signature: Signature,
}

impl Udf {
    pub fn new() -> ScalarUDF {
        ScalarUDF::new_from_impl(Self::new_inner())
    }

    fn new_inner() -> Self {
        Self {
            signature: Signature {
                type_signature: TypeSignature::Variadic(vec![DataType::Binary]),
                volatility: Volatility::Immutable,
            },
        }
    }
}

impl ScalarUDFImpl for Udf {
    fn as_any(&self) -> &dyn Any {
        &*self
    }

    fn name(&self) -> &str {
        "len"
    }

    fn signature(&self) -> &Signature {
        &self.signature
    }

    fn return_type(&self, _arg_types: &[DataType]) -> Result<DataType> {
        Ok(DataType::Int64)
    }

    fn invoke(&self, args: &[ColumnarValue]) -> Result<ColumnarValue> {
        match &args[0] {
            ColumnarValue::Array(data) => Ok(ColumnarValue::Array(match data.data_type() {
                DataType::Binary => Arc::new(
                    data.as_binary::<i32>()
                        .offsets()
                        .iter()
                        .copied()
                        .tuple_windows()
                        .map(|(a, b)| (b - a) as i64)
                        .collect::<Int64Array>(),
                ),
                DataType::LargeBinary => Arc::new(
                    data.as_binary::<i64>()
                        .offsets()
                        .iter()
                        .copied()
                        .tuple_windows()
                        .map(|(a, b)| b - a)
                        .collect::<Int64Array>(),
                ),
                _ => unreachable!(),
            })),
            ColumnarValue::Scalar(data) => {
                Ok(ColumnarValue::Scalar(ScalarValue::Int64(match data {
                    ScalarValue::Binary(b) | ScalarValue::LargeBinary(b) => {
                        b.as_ref().map(|b| b.len() as _)
                    }
                    _ => unreachable!(),
                })))
            }
        }
    }
}
