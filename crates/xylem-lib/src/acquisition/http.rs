use super::util::io_error;
use super::*;

pub(super) fn request_error(action: &'static str, url: &str, source: reqwest::Error) -> AcquisitionError {
    AcquisitionError::Request {
        action,
        remote: redact_url(url),
        reason: source.to_string(),
    }
}
pub(super) fn stream_response(
    response: Response,
    url: &str,
    destination: &Path,
    maximum: u64,
    expected: Option<&str>,
) -> Result<(u64, String), AcquisitionError> {
    let status = response.status();
    let encoded = response
        .headers()
        .get(CONTENT_ENCODING)
        .is_some_and(|value| value.to_str().map_or(true, |value| !value.eq_ignore_ascii_case("identity")));
    let declared = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let validation = match (status.is_success(), encoded, declared.filter(|bytes| *bytes > maximum)) {
        | (false, _, _) => Err(AcquisitionError::Request {
            action: "download remote source",
            remote: redact_url(url),
            reason: format!("HTTP {status}"),
        }),
        | (_, true, _) => Err(AcquisitionError::Request {
            action: "download remote source",
            remote: redact_url(url),
            reason: "encoded responses are not accepted".to_string(),
        }),
        | (_, _, Some(observed)) => Err(AcquisitionError::Limit {
            limit: "HTTP response bytes",
            maximum,
            observed,
        }),
        | (true, false, None) => Ok(()),
    };
    validation.and_then(|()| {
        File::create(destination)
            .map_err(|source| io_error("create acquisition payload", destination, source))
            .and_then(|file| {
                let mut hasher = Sha256::new();
                let mut reader = BufReader::new(response);
                let mut writer = BufWriter::new(file);
                let mut buffer = [0_u8; 64 * 1024];
                let mut total = 0_u64;
                let streamed = loop {
                    match reader.read(&mut buffer) {
                        | Ok(0) => break Ok(total),
                        | Ok(count) => {
                            total = total.saturating_add(count as u64);
                            if total > maximum {
                                break Err(AcquisitionError::Limit {
                                    limit: "HTTP response bytes",
                                    maximum,
                                    observed: total,
                                });
                            }
                            hasher.update(&buffer[..count]);
                            if let Err(source) = writer.write_all(&buffer[..count]) {
                                break Err(io_error("write acquisition payload", destination, source));
                            }
                        }
                        | Err(source) => break Err(io_error("read acquisition response", destination, source)),
                    }
                };
                streamed
                    .and_then(|total| match declared {
                        | Some(declared) if declared != total => Err(AcquisitionError::Request {
                            action: "download remote source",
                            remote: redact_url(url),
                            reason: format!("Content-Length declared {declared} bytes but received {total}"),
                        }),
                        | _ => Ok(total),
                    })
                    .and_then(|total| {
                        writer
                            .flush()
                            .map_err(|source| io_error("flush acquisition payload", destination, source))
                            .and_then(|()| {
                                writer
                                    .get_ref()
                                    .sync_all()
                                    .map_err(|source| io_error("sync acquisition payload", destination, source))
                            })
                            .and_then(|()| {
                                let calculated = format!("sha256:{:x}", hasher.finalize());
                                match expected {
                                    | Some(expected) if calculated != expected => Err(AcquisitionError::Sha256Mismatch {
                                        calculated,
                                        expected: expected.to_string(),
                                        remote: redact_url(url),
                                    }),
                                    | Some(_) | None => Ok((total, calculated)),
                                }
                            })
                    })
            })
    })
}
