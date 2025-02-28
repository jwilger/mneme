kurrent:
  docker run --name esdb-node --rm -it -p 2113:2113 docker.eventstore.com/eventstore/eventstoredb-ee --insecure --run-projections=All --enable-atom-pub-over-http
