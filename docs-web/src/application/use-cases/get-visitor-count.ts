import type { VisitorRepository } from "@/application/ports/visitor-repository";

export class GetVisitorCount {
  public constructor(private readonly repository: VisitorRepository) {}

  public execute(): Promise<number> {
    return this.repository.getQualifiedVisitorCount();
  }
}
