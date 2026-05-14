use crate::bootstrap::SystemClock;
use ora_application::{
    ApplicationError, CreateProjectHandler, DeleteProjectHandler, GetProjectHandler,
    ListProjectsHandler, UpdateProjectHandler, UuidProjectIdGenerator,
};
use ora_contracts::{
    CreateProjectRequest, CreateProjectResponse, DeleteProjectRequest, DeleteProjectResponse,
    GetProjectRequest, GetProjectResponse, ListProjectsRequest, ListProjectsResponse,
    UpdateProjectRequest, UpdateProjectResponse,
};
use ora_db::{RepositoryPool, SqliteProjectRepository};

/// Groups the transport-facing project entry points for the web adapter.
pub struct ProjectApi {
    create_project:
        CreateProjectHandler<SqliteProjectRepository, UuidProjectIdGenerator, SystemClock>,
    get_project: GetProjectHandler<SqliteProjectRepository>,
    list_projects: ListProjectsHandler<SqliteProjectRepository>,
    update_project: UpdateProjectHandler<SqliteProjectRepository, SystemClock>,
    delete_project: DeleteProjectHandler<SqliteProjectRepository, SystemClock>,
}

impl ProjectApi {
    /// Builds the project transport API from the shared repository pool and clock source.
    pub fn new(pool: RepositoryPool, clock: SystemClock) -> Self {
        let repository = SqliteProjectRepository::new(pool);

        Self {
            create_project: CreateProjectHandler::new(
                repository.clone(),
                UuidProjectIdGenerator::new(),
                clock,
            ),
            get_project: GetProjectHandler::new(repository.clone()),
            list_projects: ListProjectsHandler::new(repository.clone()),
            update_project: UpdateProjectHandler::new(repository.clone(), clock),
            delete_project: DeleteProjectHandler::new(repository, clock),
        }
    }

    /// Accepts a create-project request and delegates the use case to the application layer.
    pub fn create_project(
        &self,
        request: CreateProjectRequest,
    ) -> Result<CreateProjectResponse, ApplicationError> {
        self.create_project.handle(request)
    }

    /// Accepts a get-project request and delegates the use case to the application layer.
    pub fn get_project(
        &self,
        request: GetProjectRequest,
    ) -> Result<GetProjectResponse, ApplicationError> {
        self.get_project.handle(request)
    }

    /// Accepts a list-projects request and delegates the use case to the application layer.
    pub fn list_projects(
        &self,
        request: ListProjectsRequest,
    ) -> Result<ListProjectsResponse, ApplicationError> {
        self.list_projects.handle(request)
    }

    /// Accepts an update-project request and delegates the use case to the application layer.
    pub fn update_project(
        &self,
        request: UpdateProjectRequest,
    ) -> Result<UpdateProjectResponse, ApplicationError> {
        self.update_project.handle(request)
    }

    /// Accepts a delete-project request and delegates the use case to the application layer.
    pub fn delete_project(
        &self,
        request: DeleteProjectRequest,
    ) -> Result<DeleteProjectResponse, ApplicationError> {
        self.delete_project.handle(request)
    }
}
