// Copyright 2026 DecOperations. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use vmm::rpc_interface::VmmAction;
use vmm::vmm_config::vfio::VfioDeviceConfig;

use super::super::parsed_request::{ParsedRequest, RequestError, checked_id};
use super::{Body, StatusCode};

pub(crate) fn parse_put_vfio(
    body: &Body,
    id_from_path: Option<&str>,
) -> Result<ParsedRequest, RequestError> {
    let id = if let Some(id) = id_from_path {
        checked_id(id)?
    } else {
        return Err(RequestError::EmptyID);
    };

    let cfg = serde_json::from_slice::<VfioDeviceConfig>(body.raw())?;
    if id != cfg.vfio_dev_id.as_str() {
        return Err(RequestError::Generic(
            StatusCode::BadRequest,
            format!(
                "The id from the path [{}] does not match the id from the body [{}]!",
                id,
                cfg.vfio_dev_id.as_str()
            ),
        ));
    }
    Ok(ParsedRequest::new_sync(VmmAction::InsertVfioDevice(cfg)))
}
