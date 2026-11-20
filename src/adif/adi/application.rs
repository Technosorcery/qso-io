use crate::adif::DataType;

pub(super) struct ApplicationTypeConflict<T> {
    pub established: DataType,
    pub supplied: DataType,
    pub first: T,
}

pub(super) fn resolve_application_type<T>(
    established: Option<(DataType, T)>,
    supplied: Option<DataType>,
) -> Result<DataType, ApplicationTypeConflict<T>> {
    match (established, supplied) {
        (Some((established, first)), Some(supplied)) if established != supplied => {
            Err(ApplicationTypeConflict {
                established,
                supplied,
                first,
            })
        }
        (Some((data_type, _)), _) | (None, Some(data_type)) => Ok(data_type),
        (None, None) => Ok(DataType::MultilineString),
    }
}
