use crate::bootstrap::SystemClock;
use ora_application::{
    ApplicationError, CreateWorktreeHandler, DeleteWorktreeHandler, GetWorktreeHandler,
    ListWorktreesHandler, UpdateWorktreeHandler, UuidWorktreeIdGenerator,
};
use ora_contracts::{
    CreateWorktreeRequest, CreateWorktreeResponse, DeleteWorktreeRequest, DeleteWorktreeResponse,
    GetWorktreeRequest, GetWorktreeResponse, ListWorktreesRequest, ListWorktreesResponse,
    UpdateWorktreeRequest, UpdateWorktreeResponse,
};
use ora_db::{RepositoryPool, SqliteWorktreeRepository};

/// Groups the transport-facing worktree entry points for the web adapter.
pub struct WorktreeApi {
    create_worktree:
        CreateWorktreeHandler<SqliteWorktreeRepository, UuidWorktreeIdGenerator, SystemClock>,
    get_worktree: GetWorktreeHandler<SqliteWorktreeRepository>,
    list_worktrees: ListWorktreesHandler<SqliteWorktreeRepository>,
    update_worktree: UpdateWorktreeHandler<SqliteWorktreeRepository, SystemClock>,
    delete_worktree: DeleteWorktreeHandler<SqliteWorktreeRepository, SystemClock>,
}

impl WorktreeApi {
    /// Builds the worktree transport API from the shared repository pool and clock source.
    pub fn new(pool: RepositoryPool, clock: SystemClock) -> Self {
        let repository = SqliteWorktreeRepository::new(pool);

        Self {
            create_worktree: CreateWorktreeHandler::new(
                repository.clone(),
                UuidWorktreeIdGenerator::new(),
                clock,
            ),
            get_worktree: GetWorktreeHandler::new(repository.clone()),
            list_worktrees: ListWorktreesHandler::new(repository.clone()),
            update_worktree: UpdateWorktreeHandler::new(repository.clone(), clock),
            delete_worktree: DeleteWorktreeHandler::new(repository, clock),
        }
    }

    /// Accepts a create-worktree request and delegates the use case to the application layer.
    pub fn create_worktree(
        &self,
        request: CreateWorktreeRequest,
    ) -> Result<CreateWorktreeResponse, ApplicationError> {
        self.create_worktree.handle(request)
    }

    /// Accepts a get-worktree request and delegates the use case to the application layer.
    pub fn get_worktree(
        &self,
        request: GetWorktreeRequest,
    ) -> Result<GetWorktreeResponse, ApplicationError> {
        self.get_worktree.handle(request)
    }

    /// Accepts a list-worktrees request and delegates the use case to the application layer.
    pub fn list_worktrees(
        &self,
        request: ListWorktreesRequest,
    ) -> Result<ListWorktreesResponse, ApplicationError> {
        self.list_worktrees.handle(request)
    }

    /// Accepts an update-worktree request and delegates the use case to the application layer.
    pub fn update_worktree(
        &self,
        request: UpdateWorktreeRequest,
    ) -> Result<UpdateWorktreeResponse, ApplicationError> {
        self.update_worktree.handle(request)
    }

    /// Accepts a delete-worktree request and delegates the use case to the application layer.
    pub fn delete_worktree(
        &self,
        request: DeleteWorktreeRequest,
    ) -> Result<DeleteWorktreeResponse, ApplicationError> {
        self.delete_worktree.handle(request)
    }
}
