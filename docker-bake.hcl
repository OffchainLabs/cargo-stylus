target "default" {
  context = "."
  dockerfile = "Dockerfile"
  tags = ["offchainlabs/cargo-stylus-base:0.6.1"]
  platform = ["linux/amd64"]
}
