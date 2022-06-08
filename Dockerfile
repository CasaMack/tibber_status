FROM alpine as stripper

RUN apk add binutils
RUN apk --no-cache add ca-certificates

COPY tibber_status /tibber_status
RUN strip /tibber_status

FROM scratch as run

COPY --from=stripper /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=stripper /tibber_status /tibber_status

CMD ["/tibber_status"]
