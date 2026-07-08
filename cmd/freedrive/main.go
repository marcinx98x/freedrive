package main

import (
	"context"
	"embed"
	"fmt"
	"log"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/abdullaabdullazade/freedrive/internal/adminsettings"
	"github.com/abdullaabdullazade/freedrive/internal/api"
	"github.com/abdullaabdullazade/freedrive/internal/config"
	"github.com/abdullaabdullazade/freedrive/internal/repository/sqlite"
	"github.com/abdullaabdullazade/freedrive/internal/service"
	"github.com/abdullaabdullazade/freedrive/internal/storage"
)

//go:embed all:web
var webFS embed.FS

func main() {
	// Load configuration
	cfg, err := config.Load()
	if err != nil {
		log.Fatalf("Failed to load config: %v", err)
	}

	// Initialize database
	db, err := sqlite.New(cfg.DataDir)
	if err != nil {
		log.Fatalf("Failed to initialize database: %v", err)
	}
	defer db.Close()

	// Run migrations
	if err := db.Migrate(); err != nil {
		log.Fatalf("Failed to run migrations: %v", err)
	}
	log.Println("✓ Database migrations applied")

	adminsettings.SetDataDir(cfg.DataDir)

	// Initialize storage
	diskStorage, err := storage.NewDiskStorage(cfg.DataDir)
	if err != nil {
		log.Fatalf("Failed to initialize storage: %v", err)
	}

	// Initialize repositories
	userRepo := sqlite.NewUserRepo(db)
	fileRepo := sqlite.NewFileRepo(db)
	folderRepo := sqlite.NewFolderRepo(db)
	computerRepo := sqlite.NewComputerRepo(db)
	activityRepo := sqlite.NewActivityRepo(db)
	searchRepo := sqlite.NewSearchRepo(db)
	approvalRepo := sqlite.NewApprovalRepo(db)

	// Initialize services
	authService := service.NewAuthService(userRepo, cfg.JWTSecret)
	fileService := service.NewFileService(fileRepo, userRepo, diskStorage, activityRepo)
	computerService := service.NewComputerService(computerRepo, folderRepo)
	folderService := service.NewFolderService(folderRepo, fileRepo, userRepo, diskStorage, activityRepo, computerRepo)

	// Create admin user if no users exist
	if err := authService.EnsureAdmin(context.Background(), cfg.AdminEmail, cfg.AdminPassword); err != nil {
		log.Printf("Warning: Could not create admin user: %v", err)
	} else {
		count, _ := userRepo.Count(context.Background())
		if count == 1 {
			log.Printf("✓ Admin user created: %s", cfg.AdminEmail)
		}
	}

	// Start background tasks
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	fileService.StartTrashPurge(ctx)

	// Create router
	router := api.NewRouter(
		webFS,
		authService,
		fileService,
		folderService,
		computerService,
		fileRepo,
		userRepo,
		activityRepo,
		searchRepo,
		approvalRepo,
		diskStorage,
		cfg.MaxUploadBytes,
		cfg.DataDir,
	)

	// Start server
	server := &http.Server{
		Addr:         fmt.Sprintf(":%d", cfg.Port),
		Handler:      router,
		ReadTimeout:  5 * time.Minute,
		WriteTimeout: 10 * time.Minute,
		IdleTimeout:  120 * time.Second,
	}

	// Graceful shutdown
	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)

	go func() {
		log.Printf("╔══════════════════════════════════════════╗")
		log.Printf("║           🚀 FreeDrive v1.0.0           ║")
		log.Printf("║   Your cloud. Your rules. Your server.  ║")
		log.Printf("╠══════════════════════════════════════════╣")
		log.Printf("║   Server: http://localhost:%d            ║", cfg.Port)
		log.Printf("║   Data:   %s                            ║", cfg.DataDir)
		log.Printf("╚══════════════════════════════════════════╝")

		if err := server.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			log.Fatalf("Server error: %v", err)
		}
	}()

	<-quit
	log.Println("Shutting down server...")

	shutdownCtx, shutdownCancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer shutdownCancel()

	if err := server.Shutdown(shutdownCtx); err != nil {
		log.Fatalf("Server forced shutdown: %v", err)
	}

	cancel() // Stop background tasks
	log.Println("Server stopped gracefully")
}
