use crate::proto::rpc::lookr_server::Lookr;
use crate::proto::rpc::{QueryReq, QueryResp};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema};
use tantivy::Index;
use tonic::{Request, Response, Status};

pub(crate) struct LookrService {
    index: Index,
    query_parser: QueryParser,
    field_path: Field,
}

impl LookrService {
    pub fn new(index: Index, schema: Schema) -> Self {
        let field_path = schema.get_field(crate::indexer::FIELD_PATH).unwrap();
        let query_parser = QueryParser::for_index(&index, vec![field_path]);
        LookrService {
            index,
            query_parser,
            field_path,
        }
    }
}

#[tonic::async_trait]
impl Lookr for LookrService {
    async fn query(&self, req: Request<QueryReq>) -> Result<Response<QueryResp>, Status> {
        let query = &req.get_ref().query;

        let results = {
            let searcher = match self.index.reader() {
                Ok(r) => r.searcher(),
                Err(e) => {
                    error!("{}", e);
                    return Err(Status::internal(format!("Index reader error: {}", e)));
                }
            };

            let query_promo = match self.query_parser.parse_query(query) {
                Ok(q) => q,
                Err(e) => {
                    error!("{}", e);
                    return Err(Status::internal(format!("Could not parse query: {}", e)));
                }
            };

            let top_docs: Vec<(f32, tantivy::DocAddress)> =
                match searcher.search(&query_promo, &TopDocs::with_limit(1000)) {
                    Ok(r) => r,
                    Err(e) => {
                        error!("{}", e);
                        return Err(Status::internal(format!("Could not search: {}", e)));
                    }
                };
            let mut results = Vec::with_capacity(top_docs.len());

            for (_, doc_addr) in top_docs {
                match searcher.doc(doc_addr) {
                    Ok(d) => {
                        // TODO: fix, like, all of this...
                        match d.get_first(self.field_path).unwrap() {
                            tantivy::schema::Value::Str(s) => {
                                results.push(s.clone());
                            }
                            _ => (),
                        }
                    }
                    Err(e) => {
                        error!(
                            "Could not load DocAddress ({:?}) from searcher: {}",
                            doc_addr, e
                        );
                    }
                }
            }

            results
        };

        debug!("Query: {:?} => {} results", query, results.len());
        let resp = QueryResp { results };

        Ok(Response::new(resp))
    }
}
