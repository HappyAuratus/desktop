use crate::bootstrap::SystemClock;
use ora_application::{
    ApplicationError, CreateTaskHandler, DeleteTaskHandler, GetTaskHandler, ListTasksHandler,
    UpdateTaskHandler, UuidTaskIdGenerator,
};
use ora_contracts::{
    CreateTaskRequest, CreateTaskResponse, DeleteTaskRequest, DeleteTaskResponse, GetTaskRequest,
    GetTaskResponse, ListTasksRequest, ListTasksResponse, UpdateTaskRequest, UpdateTaskResponse,
};
use ora_db::{RepositoryPool, SqliteTaskRepository};

/// Groups the transport-facing task entry points for the web adapter.
pub struct TaskApi {
    create_task: CreateTaskHandler<SqliteTaskRepository, UuidTaskIdGenerator, SystemClock>,
    get_task: GetTaskHandler<SqliteTaskRepository>,
    list_tasks: ListTasksHandler<SqliteTaskRepository>,
    update_task: UpdateTaskHandler<SqliteTaskRepository, SystemClock>,
    delete_task: DeleteTaskHandler<SqliteTaskRepository, SystemClock>,
}

impl TaskApi {
    /// Builds the task transport API from the shared repository pool and clock source.
    pub fn new(pool: RepositoryPool, clock: SystemClock) -> Self {
        let repository = SqliteTaskRepository::new(pool);

        Self {
            create_task: CreateTaskHandler::new(
                repository.clone(),
                UuidTaskIdGenerator::new(),
                clock,
            ),
            get_task: GetTaskHandler::new(repository.clone()),
            list_tasks: ListTasksHandler::new(repository.clone()),
            update_task: UpdateTaskHandler::new(repository.clone(), clock),
            delete_task: DeleteTaskHandler::new(repository, clock),
        }
    }

    /// Accepts a create-task request and delegates the use case to the application layer.
    pub fn create_task(
        &self,
        request: CreateTaskRequest,
    ) -> Result<CreateTaskResponse, ApplicationError> {
        self.create_task.handle(request)
    }

    /// Accepts a get-task request and delegates the use case to the application layer.
    pub fn get_task(&self, request: GetTaskRequest) -> Result<GetTaskResponse, ApplicationError> {
        self.get_task.handle(request)
    }

    /// Accepts a list-tasks request and delegates the use case to the application layer.
    pub fn list_tasks(
        &self,
        request: ListTasksRequest,
    ) -> Result<ListTasksResponse, ApplicationError> {
        self.list_tasks.handle(request)
    }

    /// Accepts an update-task request and delegates the use case to the application layer.
    pub fn update_task(
        &self,
        request: UpdateTaskRequest,
    ) -> Result<UpdateTaskResponse, ApplicationError> {
        self.update_task.handle(request)
    }

    /// Accepts a delete-task request and delegates the use case to the application layer.
    pub fn delete_task(
        &self,
        request: DeleteTaskRequest,
    ) -> Result<DeleteTaskResponse, ApplicationError> {
        self.delete_task.handle(request)
    }
}
