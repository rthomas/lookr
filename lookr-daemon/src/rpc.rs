use crate::proto::rpc::lookr_server::Lookr;
use crate::proto::rpc::{QueryReq, QueryResp};
use tonic::{Request, Response, Status};

#[derive(Debug)]
pub struct LookrService {}

impl LookrService {
    pub fn new() -> Self {
        LookrService{}
    }
}

#[tonic::async_trait]
impl Lookr for LookrService {
    async fn query(&self, req: Request<QueryReq>) -> Result<Response<QueryResp>, Status> {
        let resp = QueryResp {
            results: Vec::new(),
        };

        Ok(Response::new(resp))
    }
}
