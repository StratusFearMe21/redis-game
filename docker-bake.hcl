target "docker-metadata-action-redis-game" {}

group "default" {
  targets = ["redis-game"]
}

target "redis-game" {
  inherits = ["docker-metadata-action-redis-game"]
  context = "."
}
