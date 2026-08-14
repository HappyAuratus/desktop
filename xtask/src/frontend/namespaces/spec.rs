//! Endpoint declarations for the spec generated-client namespace.

use crate::frontend::{FrontendEndpoint, FrontendHttpMethod, NO_PATH_PARAMS};
use ora_contracts::{SPEC_CATALOG_PATH, SPEC_READ_PATH, SPEC_WATCH_PATH};

const NAMESPACE: &str = "spec";

pub(super) const ENDPOINTS: &[FrontendEndpoint] = &[
    FrontendEndpoint {
        operation_name: "getSpecCatalog",
        namespace: NAMESPACE,
        member_name: "catalog",
        method: FrontendHttpMethod::Post,
        path_template: SPEC_CATALOG_PATH,
        request_type: "GetSpecCatalogRequest",
        response_type: "SpecCatalogResponse",
        path_params: NO_PATH_PARAMS,
        has_json_body: true,
    },
    FrontendEndpoint {
        operation_name: "readSpec",
        namespace: NAMESPACE,
        member_name: "read",
        method: FrontendHttpMethod::Post,
        path_template: SPEC_READ_PATH,
        request_type: "ReadSpecRequest",
        response_type: "ReadSpecResponse",
        path_params: NO_PATH_PARAMS,
        has_json_body: true,
    },
    FrontendEndpoint {
        operation_name: "watchSpecs",
        namespace: NAMESPACE,
        member_name: "watch",
        method: FrontendHttpMethod::Post,
        path_template: SPEC_WATCH_PATH,
        request_type: "WatchSpecsRequest",
        response_type: "WorkspaceFileEventBatch",
        path_params: NO_PATH_PARAMS,
        has_json_body: true,
    },
];
