import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

export class ComposeHarness {
  public constructor(private readonly workingDirectory: string) {}

  public async up(services: string[]): Promise<void> {
    await this.run(["up", "--build", "-d", ...services]);
  }

  public async publishedPorts(service: string): Promise<string[]> {
    try {
      const { stdout } = await this.run(["port", service]);
      return stdout.split("\n").filter(Boolean);
    } catch {
      return [];
    }
  }

  public async down(): Promise<void> {
    await this.run(["down", "--volumes", "--remove-orphans"]);
  }

  private run(args: string[]) {
    return execFileAsync("docker", ["compose", ...args], { cwd: this.workingDirectory });
  }
}
