class Xylem < Formula
  desc "Digital xylem that transports data through workflows with structure and support"
  homepage "https://xylem.ornl.gov"
  url "https://code.ornl.gov/research-enablement/xylem/-/archive/v__VERSION__/xylem-v__VERSION__.tar.gz"
  sha256 "__SHA256__"
  license "MIT"
  head "https://code.ornl.gov/research-enablement/xylem.git", branch: "main"

  depends_on "rust" => :build

  def fetch
    system "cargo", "fetch", "--locked"
  end

  def install
    system "cargo", "install", *std_cargo_args(path: "xylem-cli")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/xylem --version")

    (testpath/"config.json").write <<~JSON
      {
        "version": "#{version}",
        "requirements": [],
        "modules": []
      }
    JSON

    output = shell_output("#{bin}/xylem -vv run --offline --config #{testpath}/config.json 2>&1")
    assert_match "NO INPUTS", output
  end
end
