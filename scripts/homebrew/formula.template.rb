class Xylem < Formula
  desc "Digital xylem that transports data through workflows with structure and support"
  homepage "https://xylem.ornl.gov"
  url "https://static.crates.io/crates/xylem-cli/xylem-cli-__VERSION__.crate"
  sha256 "__SHA256__"
  license "MIT"
  head "https://code.ornl.gov/research-enablement/xylem.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: ".")
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

    output = shell_output("#{bin}/xylem -vv run #{testpath}/config.json --offline 2>&1")
    assert_match "NO INPUTS", output
  end
end
