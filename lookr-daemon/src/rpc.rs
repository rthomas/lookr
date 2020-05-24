use crate::index::Index;
use crate::proto::rpc::lookr_server::Lookr;
use crate::proto::rpc::{QueryReq, QueryResp};
use std::sync::{Arc, Mutex};
use tonic::{Request, Response, Status};

#[derive(Debug)]
pub(crate) struct LookrService {
    index: Arc<Mutex<Index>>,
}

impl LookrService {
    pub fn new(index: Arc<Mutex<Index>>) -> Self {
        LookrService { index }
    }
}

#[tonic::async_trait]
impl Lookr for LookrService {
    async fn query(&self, req: Request<QueryReq>) -> Result<Response<QueryResp>, Status> {
        let query = &req.get_ref().query;

        let results = {
            let i = self.index.lock().unwrap();
            match i.query(query) {
                Ok(r) => r,
                Err(e) => {
                    return Err(Status::internal(format!("{}", e)));
                }
            }
        };

        debug!("Query: {:?} => {} results", query, results.len());
        let resp = QueryResp { results };

        Ok(Response::new(resp))
    }
}
